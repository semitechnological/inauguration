use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use thiserror::Error;
use tokio::fs::OpenOptions;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::sync::{Mutex, mpsc};
use tokio::time::sleep;

use super::generated_protocol::PatchType;
use crate::hybrid_sil;
use crate::native_swift_sil;
use crate::parser_registry::{self, ParserCli, ResolvedBuildParser};
use crate::sil_emit;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReloadPatch {
    pub target: String,
    pub patch_type: PatchType,
    pub compatible: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PatchEnvelope {
    pub protocol_version: u8,
    pub patch_id: String,
    pub timestamp_ms: u64,
    pub patch: ReloadPatch,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeMetric {
    pub timestamp_ms: u64,
    pub source: String,
    pub target: String,
    pub compatible: bool,
    pub reason: String,
    pub compile_check_ms: u64,
    pub compile_cache_hit: bool,
    /// Count of `function_ref` edges from [`hybrid_sil::extract_call_graph`] when in-tree subset SIL exists (no `swiftc`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sil_call_edges: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct DaemonConfig {
    pub watch_root: PathBuf,
    pub socket_path: PathBuf,
    pub metrics_path: PathBuf,
    pub debounce_ms: u64,
}

#[derive(Debug, Error)]
pub enum DaemonError {
    #[error("notify error: {0}")]
    Notify(#[from] notify::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Clone)]
struct ClientPool {
    writers: Arc<Mutex<Vec<tokio::net::unix::OwnedWriteHalf>>>,
}

impl ClientPool {
    fn new() -> Self {
        Self {
            writers: Arc::new(Mutex::new(Vec::new())),
        }
    }

    async fn add(&self, writer: tokio::net::unix::OwnedWriteHalf) {
        self.writers.lock().await.push(writer);
    }

    async fn broadcast_line(&self, line: &str) -> Result<(), DaemonError> {
        let mut stale_indexes = Vec::new();
        let mut writers = self.writers.lock().await;
        for (index, writer) in writers.iter_mut().enumerate() {
            if writer.write_all(line.as_bytes()).await.is_err()
                || writer.write_all(b"\n").await.is_err()
                || writer.flush().await.is_err()
            {
                stale_indexes.push(index);
            }
        }
        for index in stale_indexes.into_iter().rev() {
            writers.remove(index);
        }
        Ok(())
    }
}

fn now_ms() -> u64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(dur) => dur.as_millis() as u64,
        Err(_) => 0,
    }
}

pub fn patch_id_for(path: &str) -> String {
    format!("{}-{}", now_ms(), path.replace('/', "_"))
}

pub fn plan_patch(path: &str, changed_symbols: &[String]) -> ReloadPatch {
    plan_patch_with_sil_graph(path, changed_symbols, None)
}

/// Plan reload patch shape. When **`sil_call_edges`** is **`Some(n)`** with **`n > 0`** (subset SIL had `function_ref` edges) and the path is not **`App.swift`**, **`FullModule`** downgrades to **`ViewBody`** so preview can try hot patch instead of forced restart.
pub fn plan_patch_with_sil_graph(
    path: &str,
    changed_symbols: &[String],
    sil_call_edges: Option<u32>,
) -> ReloadPatch {
    let mut patch_type = if changed_symbols.iter().any(|s| s.contains("body")) {
        PatchType::ViewBody
    } else if changed_symbols.iter().any(|s| s.contains("modifier")) {
        PatchType::Modifier
    } else {
        PatchType::FullModule
    };
    if matches!(patch_type, PatchType::FullModule)
        && sil_call_edges.is_some_and(|n| n > 0)
        && !path.ends_with("App.swift")
    {
        patch_type = PatchType::ViewBody;
    }
    let compatible = !path.ends_with("App.swift") && !matches!(patch_type, PatchType::FullModule);
    ReloadPatch {
        target: path.to_string(),
        patch_type,
        compatible,
    }
}

pub fn symbols_for_path(path: &str) -> Vec<String> {
    if path.contains("ContentView") {
        vec!["body".to_string()]
    } else if path.contains("Modifier") {
        vec!["modifier".to_string()]
    } else {
        vec![]
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct NodeId(String);

impl NodeId {
    fn from_path(path: &str) -> Self {
        Self(path.to_string())
    }
}

#[derive(Debug, Clone, Default)]
struct QueryGraph {
    symbols: HashMap<NodeId, Vec<String>>,
    deps: HashMap<NodeId, HashSet<NodeId>>,
    reverse_deps: HashMap<NodeId, HashSet<NodeId>>,
}

impl QueryGraph {
    fn upsert_node(&mut self, path: &str, symbols: Vec<String>, deps: HashSet<String>) {
        let node = NodeId::from_path(path);
        if let Some(old) = self.deps.remove(&node) {
            for dep in old {
                if let Some(reverse) = self.reverse_deps.get_mut(&dep) {
                    reverse.remove(&node);
                }
            }
        }
        self.symbols.insert(node.clone(), symbols);
        let mapped: HashSet<NodeId> = deps.into_iter().map(NodeId).collect();
        for dep in &mapped {
            self.reverse_deps
                .entry(dep.clone())
                .or_default()
                .insert(node.clone());
        }
        self.deps.insert(node, mapped);
    }

    fn invalidate_from_path(&self, path: &str) -> Vec<NodeId> {
        let start = NodeId::from_path(path);
        let mut queue = VecDeque::from([start.clone()]);
        let mut seen = HashSet::from([start]);
        let mut dirty = Vec::new();
        while let Some(node) = queue.pop_front() {
            dirty.push(node.clone());
            if let Some(dependents) = self.reverse_deps.get(&node) {
                for dependent in dependents {
                    if seen.insert(dependent.clone()) {
                        queue.push_back(dependent.clone());
                    }
                }
            }
        }
        dirty
    }

    fn symbols_for_node(&self, node: &NodeId) -> &[String] {
        self.symbols.get(node).map(Vec::as_slice).unwrap_or(&[])
    }
}

fn infer_deps_for_path(path: &str) -> HashSet<String> {
    let mut deps = HashSet::new();
    if path.ends_with("App.swift") {
        return deps;
    }
    if let Some((prefix, _)) = path.rsplit_once('/') {
        deps.insert(format!("{prefix}/App.swift"));
    } else {
        deps.insert("App.swift".to_string());
    }
    deps
}

fn classify_change(path: &str, graph: &mut QueryGraph) -> Vec<String> {
    let local_symbols = symbols_for_path(path);
    graph.upsert_node(path, local_symbols, infer_deps_for_path(path));
    let dirty_nodes = graph.invalidate_from_path(path);
    let mut combined = Vec::new();
    for node in dirty_nodes {
        combined.extend_from_slice(graph.symbols_for_node(&node));
    }
    combined.sort_unstable();
    combined.dedup();
    combined
}

fn compile_check_swift(path: &Path) -> bool {
    sil_emit::compile_check_swift_path(path)
}

/// Best-effort gate before emitting a reload patch: Swift uses [`sil_emit::compile_check_swift_path`]
/// (**`IN_NATIVE_SWIFT_SIL`**: in-tree subset first on `try`/`only`, else **`swiftc -typecheck`**); Core IR paths use
/// [`parser_registry::parse_with_resolved`] (`.in` / `.icore` / polyglot).
/// When path is Swift and sources match **`swift_subset`**, derive SIL in-process and count call-graph edges (`function_ref`).
/// Returns **`None`** if not subset-shaped (no extra `swiftc` / emit work).
fn sil_subset_call_edge_count(path: &Path) -> Option<u32> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ext != "swift" {
        return None;
    }
    let combined = sil_emit::combined_swift_sources_for_path(path).ok()?;
    let sil = native_swift_sil::try_emit_in_tree_sil(&combined, "App")?;
    let artifact = hybrid_sil::parse_textual_sil(&sil);
    let cleaned = hybrid_sil::remove_debug_insts(&artifact);
    let n = hybrid_sil::extract_call_graph(&cleaned).call_edges.len();
    Some(n as u32)
}

pub fn compile_check(path: &Path) -> bool {
    if !path.exists() {
        return false;
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ext == "swift" {
        return compile_check_swift(path);
    }
    let resolved = parser_registry::resolve_parser_id(path, ParserCli::Auto);
    match resolved {
        ResolvedBuildParser::SwiftSilEmit => compile_check_swift(path),
        ResolvedBuildParser::CoreIr(_) => parser_registry::parse_with_resolved(resolved, path)
            .map(|m| m.is_some())
            .unwrap_or(false),
    }
}

fn extension_triggers_hotreload_notify(ext: &str) -> bool {
    let el = ext.to_ascii_lowercase();
    el == "swift" || parser_registry::parser_id_from_extension(&el).is_some()
}

#[derive(Debug, Clone)]
struct CompileCacheEntry {
    modified_ms: u128,
    ok: bool,
}

type CompileCache = HashMap<String, CompileCacheEntry>;

fn modified_ms(path: &Path) -> Option<u128> {
    path.metadata()
        .ok()?
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|dur| dur.as_millis())
}

fn compile_check_cached(path: &Path, cache: &mut CompileCache) -> (bool, bool, u64) {
    let start = std::time::Instant::now();
    let key = path.to_string_lossy().to_string();
    let Some(current_ms) = modified_ms(path) else {
        return (false, false, start.elapsed().as_millis() as u64);
    };
    if let Some(entry) = cache.get(&key)
        && entry.modified_ms == current_ms
    {
        return (entry.ok, true, start.elapsed().as_millis() as u64);
    }
    let ok = compile_check(path);
    cache.insert(
        key,
        CompileCacheEntry {
            modified_ms: current_ms,
            ok,
        },
    );
    (ok, false, start.elapsed().as_millis() as u64)
}

pub fn apply_restart_supervisor(mut patch: ReloadPatch, compile_ok: bool) -> (ReloadPatch, String) {
    if !compile_ok {
        patch.compatible = false;
        return (patch, "compile_failed".to_string());
    }
    if matches!(patch.patch_type, PatchType::FullModule) || !patch.compatible {
        patch.compatible = false;
        return (patch, "restart_required".to_string());
    }
    (patch, "patch_applied".to_string())
}

pub async fn append_metric(path: &Path, metric: &RuntimeMetric) -> Result<(), DaemonError> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await?;
    let line = serde_json::to_string(metric)?;
    file.write_all(line.as_bytes()).await?;
    file.write_all(b"\n").await?;
    file.flush().await?;
    Ok(())
}

async fn start_socket_server(
    socket_path: &Path,
    pool: ClientPool,
) -> Result<tokio::task::JoinHandle<()>, DaemonError> {
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if socket_path.exists() {
        std::fs::remove_file(socket_path)?;
    }
    let listener = UnixListener::bind(socket_path)?;
    let task = tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let (reader, writer) = stream.into_split();
            pool.add(writer).await;
            tokio::spawn(async move {
                let mut lines = BufReader::new(reader).lines();
                while lines.next_line().await.ok().flatten().is_some() {}
            });
        }
    });
    Ok(task)
}

async fn emit_patch(
    target: &str,
    symbols: &[String],
    metrics_path: &Path,
    pool: &ClientPool,
    compile_cache: &mut CompileCache,
) -> Result<ReloadPatch, DaemonError> {
    let (compile_ok, compile_cache_hit, compile_check_ms) =
        compile_check_cached(Path::new(target), compile_cache);
    let sil_call_edges = if compile_ok {
        sil_subset_call_edge_count(Path::new(target))
    } else {
        None
    };
    let patch = plan_patch_with_sil_graph(target, symbols, sil_call_edges);
    let (patch, mut reason) = apply_restart_supervisor(patch, compile_ok);
    if let Some(n) = sil_call_edges {
        reason = format!("{reason}|sil_call_edges={n}");
    }
    let envelope = PatchEnvelope {
        protocol_version: 1,
        patch_id: patch_id_for(target),
        timestamp_ms: now_ms(),
        patch: patch.clone(),
        reason: reason.clone(),
    };
    let metric = RuntimeMetric {
        timestamp_ms: now_ms(),
        source: "daemon".to_string(),
        target: patch.target.clone(),
        compatible: patch.compatible,
        reason,
        compile_check_ms,
        compile_cache_hit,
        sil_call_edges,
    };
    append_metric(metrics_path, &metric).await?;
    let line = serde_json::to_string(&envelope)?;
    pool.broadcast_line(&line).await?;
    Ok(patch)
}

pub async fn run_daemon(config: DaemonConfig) -> Result<(), DaemonError> {
    let pool = ClientPool::new();
    let _server = start_socket_server(&config.socket_path, pool.clone()).await?;
    let (tx, mut rx) = mpsc::channel::<String>(128);
    let tx_watch = tx.clone();
    let watch_root = config.watch_root.clone();

    let mut watcher: RecommendedWatcher =
        notify::recommended_watcher(move |result: notify::Result<Event>| {
            if let Ok(event) = result {
                for path in event.paths {
                    let notify = path
                        .extension()
                        .and_then(|ext| ext.to_str())
                        .map(extension_triggers_hotreload_notify)
                        .unwrap_or(false);
                    if notify {
                        let _ = tx_watch.blocking_send(path.to_string_lossy().to_string());
                    }
                }
            }
        })?;
    watcher.watch(&watch_root, RecursiveMode::Recursive)?;
    let mut compile_cache: CompileCache = HashMap::new();
    let mut query_graph = QueryGraph::default();

    loop {
        match rx.recv().await {
            Some(path) => {
                let mut latest = Some(path);
                sleep(Duration::from_millis(config.debounce_ms)).await;
                while let Ok(next) = rx.try_recv() {
                    latest = Some(next);
                }
                if let Some(target) = latest.take() {
                    let symbols = classify_change(&target, &mut query_graph);
                    let _ = emit_patch(
                        &target,
                        &symbols,
                        &config.metrics_path,
                        &pool,
                        &mut compile_cache,
                    )
                    .await?;
                }
            }
            None => return Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncBufReadExt, BufReader};
    use tokio::net::UnixStream;

    #[test]
    fn app_file_forces_restart_path() {
        let patch = plan_patch("SampleApp.swift", &[]);
        assert!(!patch.compatible);
    }

    #[test]
    fn sil_graph_downgrades_full_module_to_view_body_for_hot_patch() {
        let p = plan_patch_with_sil_graph("Sources/Helper.swift", &[], Some(2));
        assert_eq!(p.patch_type, PatchType::ViewBody);
        assert!(p.compatible);
        let (out, reason) = apply_restart_supervisor(p, true);
        assert!(out.compatible);
        assert_eq!(reason, "patch_applied");
    }

    #[test]
    fn sil_graph_does_not_downgrade_app_swift() {
        let p = plan_patch_with_sil_graph("Sources/App.swift", &[], Some(5));
        assert_eq!(p.patch_type, PatchType::FullModule);
        assert!(!p.compatible);
    }

    #[test]
    fn content_view_prefers_view_body_patch() {
        let patch = plan_patch("ContentView.swift", &["body".to_string()]);
        assert_eq!(patch.patch_type, PatchType::ViewBody);
        assert!(patch.compatible);
    }

    #[test]
    fn symbol_classification_uses_path_heuristics() {
        assert_eq!(symbols_for_path("Foo/ContentView.swift"), vec!["body"]);
        assert_eq!(
            symbols_for_path("Foo/SpacingModifier.swift"),
            vec!["modifier"]
        );
        let empty: Vec<String> = vec![];
        assert_eq!(symbols_for_path("Foo/SampleApp.swift"), empty);
    }

    #[test]
    fn query_graph_invalidates_dependents() {
        let mut graph = QueryGraph::default();
        graph.upsert_node(
            "App.swift",
            vec![],
            HashSet::from(["ContentView.swift".to_string()]),
        );
        graph.upsert_node(
            "ContentView.swift",
            vec!["body".to_string()],
            HashSet::new(),
        );
        let dirty = graph.invalidate_from_path("ContentView.swift");
        assert!(dirty.contains(&NodeId("ContentView.swift".to_string())));
        assert!(dirty.contains(&NodeId("App.swift".to_string())));
    }

    #[test]
    fn classify_change_keeps_body_symbol_signal() {
        let mut graph = QueryGraph::default();
        let symbols = classify_change("Foo/ContentView.swift", &mut graph);
        assert!(symbols.contains(&"body".to_string()));
    }

    #[test]
    fn restart_supervisor_marks_restart_for_failed_compile() {
        let patch = plan_patch("ContentView.swift", &["body".to_string()]);
        let (updated, reason) = apply_restart_supervisor(patch, false);
        assert!(!updated.compatible);
        assert_eq!(reason, "compile_failed");
    }

    #[test]
    fn compile_check_cache_hits_on_unchanged_file() {
        let path = std::env::temp_dir().join(format!("compile-cache-{}.swift", now_ms()));
        std::fs::write(&path, "struct X {}").expect("write swift");
        let mut cache = CompileCache::new();
        let first = compile_check_cached(&path, &mut cache);
        let second = compile_check_cached(&path, &mut cache);
        assert!(!first.1);
        assert!(second.1);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn compile_check_rust_with_main_succeeds_via_rust_front() {
        let path = std::env::temp_dir().join(format!("compile-check-rs-{}.rs", now_ms()));
        std::fs::write(&path, "fn main() {}\n").expect("write rust");
        assert!(compile_check(&path));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn compile_check_rust_without_main_fails() {
        let path = std::env::temp_dir().join(format!("compile-check-rs-empty-{}.rs", now_ms()));
        std::fs::write(&path, "// no main\n").expect("write rust");
        assert!(!compile_check(&path));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn extension_triggers_notify_for_swift_in_and_java() {
        assert!(extension_triggers_hotreload_notify("swift"));
        assert!(extension_triggers_hotreload_notify("SWIFT"));
        assert!(extension_triggers_hotreload_notify("in"));
        assert!(extension_triggers_hotreload_notify("java"));
        assert!(!extension_triggers_hotreload_notify("md"));
    }

    #[test]
    fn patch_type_serializes_to_schema_snake_case() {
        let patch = ReloadPatch {
            target: "ContentView.swift".to_string(),
            patch_type: PatchType::ViewBody,
            compatible: true,
        };
        let json = serde_json::to_string(&patch).expect("serialize reload patch");
        assert!(json.contains("\"patch_type\":\"view_body\""));
    }

    #[tokio::test]
    async fn metric_file_is_written() {
        let path = std::env::temp_dir().join(format!("hotreload-metric-{}.ndjson", now_ms()));
        let metric = RuntimeMetric {
            timestamp_ms: now_ms(),
            source: "test".to_string(),
            target: "ContentView.swift".to_string(),
            compatible: true,
            reason: "patch_applied".to_string(),
            compile_check_ms: 1,
            compile_cache_hit: false,
            sil_call_edges: None,
        };
        append_metric(&path, &metric).await.expect("metric write");
        let content = tokio::fs::read_to_string(&path).await.expect("metric read");
        assert!(content.contains("ContentView.swift"));
        let _ = tokio::fs::remove_file(path).await;
    }

    #[test]
    fn sil_subset_call_edge_count_sees_function_ref() {
        let dir = std::env::temp_dir().join(format!("hotreload-sil-edges-{}", now_ms()));
        let _ = std::fs::create_dir_all(&dir);
        let f = dir.join("App.swift");
        std::fs::write(
            &f,
            "struct User\nfunc helper() -> Void\nfunc main(user: User) -> Void\n",
        )
        .expect("write subset swift");
        let n = sil_subset_call_edge_count(&f).expect("subset sil + graph");
        assert!(n >= 1, "expected main→helper edge, got {n}");
        let _ = std::fs::remove_file(&f);
        let _ = std::fs::remove_dir(&dir);
    }

    #[tokio::test]
    async fn socket_client_receives_patch_envelope() {
        let base = std::env::temp_dir().join(format!("hotreload-sock-{}", now_ms()));
        let socket_path = base.with_extension("sock");
        let metrics_path = base.with_extension("ndjson");
        let pool = ClientPool::new();
        let _server = start_socket_server(&socket_path, pool.clone())
            .await
            .expect("socket server starts");
        let stream = UnixStream::connect(&socket_path)
            .await
            .expect("client connects");
        let mut lines = BufReader::new(stream).lines();
        let _ = emit_patch(
            "ContentView.swift",
            &["body".to_string()],
            &metrics_path,
            &pool,
            &mut CompileCache::new(),
        )
        .await
        .expect("patch emitted");
        let line = lines
            .next_line()
            .await
            .expect("line read")
            .expect("line present");
        assert!(line.contains("ContentView.swift"));
        let _ = tokio::fs::remove_file(&socket_path).await;
        let _ = tokio::fs::remove_file(&metrics_path).await;
    }
}
