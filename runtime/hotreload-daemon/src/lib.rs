use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use thiserror::Error;
use tokio::fs::OpenOptions;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::sync::{Mutex, mpsc};
use tokio::time::sleep;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PatchType {
    ViewBody,
    Modifier,
    FullModule,
}

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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeMetric {
    pub timestamp_ms: u64,
    pub source: String,
    pub target: String,
    pub compatible: bool,
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
    let patch_type = if changed_symbols.iter().any(|s| s.contains("body")) {
        PatchType::ViewBody
    } else if changed_symbols.iter().any(|s| s.contains("modifier")) {
        PatchType::Modifier
    } else {
        PatchType::FullModule
    };
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
) -> Result<ReloadPatch, DaemonError> {
    let patch = plan_patch(target, symbols);
    let envelope = PatchEnvelope {
        protocol_version: 1,
        patch_id: patch_id_for(target),
        timestamp_ms: now_ms(),
        patch: patch.clone(),
    };
    let metric = RuntimeMetric {
        timestamp_ms: now_ms(),
        source: "daemon".to_string(),
        target: patch.target.clone(),
        compatible: patch.compatible,
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
                    let is_swift = path
                        .extension()
                        .and_then(|ext| ext.to_str())
                        .map(|ext| ext == "swift")
                        .unwrap_or(false);
                    if is_swift {
                        let _ = tx_watch.blocking_send(path.to_string_lossy().to_string());
                    }
                }
            }
        })?;
    watcher.watch(&watch_root, RecursiveMode::Recursive)?;

    loop {
        match rx.recv().await {
            Some(path) => {
                let mut latest = Some(path);
                sleep(Duration::from_millis(config.debounce_ms)).await;
                while let Ok(next) = rx.try_recv() {
                    latest = Some(next);
                }
                if let Some(target) = latest.take() {
                    let symbols = symbols_for_path(&target);
                    let _ = emit_patch(&target, &symbols, &config.metrics_path, &pool).await?;
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
    fn content_view_prefers_view_body_patch() {
        let patch = plan_patch("ContentView.swift", &["body".to_string()]);
        assert_eq!(patch.patch_type, PatchType::ViewBody);
        assert!(patch.compatible);
    }

    #[test]
    fn symbol_classification_uses_path_heuristics() {
        assert_eq!(symbols_for_path("Foo/ContentView.swift"), vec!["body"]);
        assert_eq!(symbols_for_path("Foo/SpacingModifier.swift"), vec!["modifier"]);
        let empty: Vec<String> = vec![];
        assert_eq!(symbols_for_path("Foo/SampleApp.swift"), empty);
    }

    #[tokio::test]
    async fn metric_file_is_written() {
        let path = std::env::temp_dir().join(format!("hotreload-metric-{}.ndjson", now_ms()));
        let metric = RuntimeMetric {
            timestamp_ms: now_ms(),
            source: "test".to_string(),
            target: "ContentView.swift".to_string(),
            compatible: true,
        };
        append_metric(&path, &metric).await.expect("metric write");
        let content = tokio::fs::read_to_string(&path).await.expect("metric read");
        assert!(content.contains("ContentView.swift"));
        let _ = tokio::fs::remove_file(path).await;
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
