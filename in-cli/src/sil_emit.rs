//! Extract textual SIL for native **`in build`**: optional **in-tree** subset compiler, else **`swiftc`**.
//!
//! - **`IN_NATIVE_SWIFT_SIL=try`** (or **`1`**): run the Rust subset front (`native_swift_sil` +
//!   `swift_subset`) first; on success emit SIL without **`swiftc`**, otherwise fall back to **`swiftc`**.
//! - **`IN_NATIVE_SWIFT_SIL=only`**: require the in-tree path (no **`swiftc`**).
//! - **`IN_SWIFTC`**: override the **`swiftc`** binary when falling back.
//!
//! Hot reload compile gate uses the same policy via [`compile_check_swift_path`] (including **`swift build`** + **`-I`** retry when **`swiftc -typecheck`** fails inside a package, same as emit).
//!
//! Orchestration and SIL passes stay in `in`; full Swift remains out of tree for non-subset sources.

use crate::native_swift_sil::{NativeSwiftSilMode, native_swift_sil_mode_from_env};
use serde_json::Value as JsonValue;
use std::collections::{BTreeMap, HashSet};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use thiserror::Error;

static SWIFT_SIL_TMP_SEQ: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Error)]
pub enum SilEmitError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Msg(String),
}

/// SwiftPM root `P` where `path` lives under `P/Sources/...` or `P/swift/Sources/...` and `P` has
/// `Package.swift`. Skips unrelated monorepo roots above nested examples (e.g. `examples/counter`
/// without its own manifest must not resolve to the parent repo package).
fn package_root_containing_swift_file(path: &Path) -> Option<PathBuf> {
    let mut cur = path.parent()?;
    loop {
        if cur.join("Package.swift").exists() {
            let in_sources = path.starts_with(cur.join("Sources"));
            let in_swift_sources = path.starts_with(cur.join("swift").join("Sources"));
            if (in_sources || in_swift_sources) && collect_package_sources_flexible(cur).is_ok() {
                return Some(cur.to_path_buf());
            }
        }
        cur = cur.parent()?;
    }
}

fn dedup_swift_paths(paths: &mut Vec<PathBuf>) {
    paths.sort();
    let mut seen: HashSet<PathBuf> = HashSet::new();
    paths.retain(|p| {
        let key = fs::canonicalize(p).unwrap_or_else(|_| p.clone());
        seen.insert(key)
    });
}

/// `swiftc` disambiguates private declarations across primaries using the **basename** only; two
/// files like `AST/Registration.swift` and `SIL/Registration.swift` cannot share one WMO
/// `-emit-sil` invocation. Split into unique-basename batch vs per-file groups for duplicates.
fn partition_inputs_by_basename(inputs: &[PathBuf]) -> (Vec<PathBuf>, Vec<Vec<PathBuf>>) {
    let mut groups: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
    for p in inputs {
        let name = p
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        groups.entry(name).or_default().push(p.clone());
    }
    let mut singles = Vec::new();
    let mut multis = Vec::new();
    for (_, mut g) in groups {
        g.sort();
        if g.len() == 1 {
            singles.push(g.pop().expect("one path"));
        } else {
            multis.push(g);
        }
    }
    singles.sort();
    multis.sort_by(|a, b| {
        let ba = a.first().and_then(|p| p.file_name());
        let bb = b.first().and_then(|p| p.file_name());
        ba.cmp(&bb)
    });
    (singles, multis)
}

fn collect_sources_dir(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), SilEmitError> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let p = entry?.path();
        if p.is_dir() {
            collect_sources_dir(&p, out)?;
        } else if p.extension().and_then(|s| s.to_str()) == Some("swift") {
            out.push(p);
        }
    }
    Ok(())
}

/// When `path` is under `<pkg>/Sources/`, prefer the SwiftPM-style target directory: if the first
/// path segment under `Sources/` is a directory (e.g. `Sources/Foo/Bar.swift`), collect only that
/// subtree so we do not merge unrelated targets (e.g. `SwiftPreviewHost` vs `SwiftPreviewHostClient`
/// both exporting `@main`). If the first segment is a file in `Sources/`, collect all of `Sources/`.
fn collect_package_sources_for_input_file(pkg: &Path, path: &Path) -> Result<Vec<PathBuf>, SilEmitError> {
    let sources = fs::canonicalize(pkg.join("Sources")).map_err(|_| {
        SilEmitError::Msg(format!("no Sources/ under {}", pkg.display()))
    })?;
    let rel = path.strip_prefix(&sources).map_err(|_| {
        SilEmitError::Msg(format!(
            "path {} is not under {}",
            path.display(),
            sources.display()
        ))
    })?;
    let mut out = Vec::new();
    match rel.components().next() {
        Some(std::path::Component::Normal(name)) => {
            let candidate = sources.join(name);
            if candidate.is_dir() {
                collect_sources_dir(&candidate, &mut out)?;
            } else {
                collect_sources_dir(&sources, &mut out)?;
            }
        }
        _ => collect_sources_dir(&sources, &mut out)?,
    }
    let generated = pkg.join("Generated");
    if generated.is_dir() {
        collect_sources_dir(&generated, &mut out)?;
    }
    dedup_swift_paths(&mut out);
    if out.is_empty() {
        return Err(SilEmitError::Msg(format!(
            "no .swift files under {}",
            sources.display()
        )));
    }
    Ok(out)
}

/// All `*.swift` files under `<pkg>/Sources` plus codegen under `<pkg>/Generated` when present
/// (SwiftPM targets often list both; sibling-only resolution would miss `Generated/`).
fn collect_package_sources(pkg: &Path) -> Result<Vec<PathBuf>, SilEmitError> {
    let sources = pkg.join("Sources");
    if !sources.is_dir() {
        return Err(SilEmitError::Msg(format!(
            "no Sources/ under {}",
            pkg.display()
        )));
    }
    let mut out = Vec::new();
    collect_sources_dir(&sources, &mut out)?;
    let generated = pkg.join("Generated");
    if generated.is_dir() {
        collect_sources_dir(&generated, &mut out)?;
    }
    dedup_swift_paths(&mut out);
    if out.is_empty() {
        return Err(SilEmitError::Msg(format!(
            "no .swift files under {}",
            sources.display()
        )));
    }
    Ok(out)
}

/// SwiftPM allows non-default source roots (e.g. `swift/Sources/...`).
fn collect_package_sources_flexible(pkg: &Path) -> Result<Vec<PathBuf>, SilEmitError> {
    if pkg.join("Sources").is_dir() {
        return collect_package_sources(pkg);
    }
    let swift_nested = pkg.join("swift/Sources");
    if swift_nested.is_dir() {
        let mut out = Vec::new();
        collect_sources_dir(&swift_nested, &mut out)?;
        let generated = pkg.join("Generated");
        if generated.is_dir() {
            collect_sources_dir(&generated, &mut out)?;
        }
        let generated_l = pkg.join("generated");
        if generated_l.is_dir() {
            collect_sources_dir(&generated_l, &mut out)?;
        }
        dedup_swift_paths(&mut out);
        if out.is_empty() {
            return Err(SilEmitError::Msg(format!(
                "no .swift files under {}",
                swift_nested.display()
            )));
        }
        return Ok(out);
    }
    Err(SilEmitError::Msg(format!(
        "no Sources/ or swift/Sources/ under {}",
        pkg.display()
    )))
}

fn collect_swift_siblings_in_sources_parent(path: &Path) -> Option<Vec<PathBuf>> {
    let parent = path.parent()?;
    if parent.file_name().and_then(|s| s.to_str()) != Some("Sources") {
        return None;
    }
    let mut out = Vec::new();
    let rd = fs::read_dir(parent).ok()?;
    for entry in rd.flatten() {
        let p = entry.path();
        if p.extension().and_then(|s| s.to_str()) == Some("swift") {
            out.push(p);
        }
    }
    out.sort();
    if out.is_empty() { None } else { Some(out) }
}

fn collect_swift_shallow(
    dir: &Path,
    depth: u32,
    out: &mut Vec<PathBuf>,
) -> Result<(), SilEmitError> {
    if depth == 0 {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let p = entry?.path();
        if p.is_dir() {
            collect_swift_shallow(&p, depth - 1, out)?;
        } else if p.extension().and_then(|s| s.to_str()) == Some("swift") {
            out.push(p);
        }
    }
    Ok(())
}

fn resolve_swift_inputs(path: &Path) -> Result<Vec<PathBuf>, SilEmitError> {
    if !path.exists() {
        return Err(SilEmitError::Msg(format!(
            "path does not exist: {}",
            path.display()
        )));
    }

    if path.is_file() {
        if path.extension().and_then(|s| s.to_str()) != Some("swift") {
            return Err(SilEmitError::Msg(
                "native build expects a .swift file or a directory with Swift sources".into(),
            ));
        }
        let path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        // Prefer full package source roots (Sources/, Generated/, swift/Sources/) when a
        // Package.swift exists; sibling-only sets miss SPM-listed paths outside `Sources/`.
        if let Some(pkg) = package_root_containing_swift_file(&path) {
            let pkg_sources = fs::canonicalize(pkg.join("Sources")).unwrap_or_else(|_| pkg.join("Sources"));
            if path.starts_with(&pkg_sources) {
                return collect_package_sources_for_input_file(&pkg, &path);
            }
            return collect_package_sources_flexible(&pkg);
        }
        if let Some(sibs) = collect_swift_siblings_in_sources_parent(&path) {
            return Ok(sibs);
        }
        return Ok(vec![path]);
    }

    if path.join("Package.swift").exists() {
        return collect_package_sources_flexible(path);
    }

    let mut out = Vec::new();
    collect_swift_shallow(path, 8, &mut out)?;
    out.sort();
    if out.is_empty() {
        return Err(SilEmitError::Msg(format!(
            "no .swift files under {}",
            path.display()
        )));
    }
    Ok(out)
}

fn macos_sdk_path() -> Option<String> {
    let output = Command::new("xcrun")
        .args(["--sdk", "macosx", "--show-sdk-path"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

fn swiftc_executable() -> OsString {
    std::env::var_os("IN_SWIFTC").unwrap_or_else(|| OsString::from("swiftc"))
}

fn append_extra_swiftc_args(cmd: &mut Command) {
    let Ok(flags) = std::env::var("IN_SWIFTC_FLAGS") else {
        return;
    };
    for word in flags.split_whitespace() {
        if !word.is_empty() {
            cmd.arg(word);
        }
    }
}

/// Add `-Xcc -fmodule-map-file=…` (+ `-Xcc -I`) for each `*.modulemap` in `dir`.
///
/// When `skip_default_modulemap` is true, omits `module.modulemap` so named maps (e.g. UniFFI
/// `fooFFI.modulemap`) are used without duplicating an umbrella map.
fn append_modulemap_clang_flags(cmd: &mut Command, dir: &Path, skip_default_modulemap: bool) {
    if !dir.is_dir() {
        return;
    }
    let Ok(rd) = fs::read_dir(dir) else {
        return;
    };
    let mut maps = Vec::new();
    for entry in rd.flatten() {
        let p = entry.path();
        if p.extension().and_then(|x| x.to_str()) == Some("modulemap") {
            if skip_default_modulemap
                && p.file_name().and_then(|s| s.to_str()) == Some("module.modulemap")
            {
                continue;
            }
            maps.push(p);
        }
    }
    maps.sort();
    for m in maps {
        cmd.arg("-Xcc")
            .arg(format!("-fmodule-map-file={}", m.display()));
    }
    cmd.arg("-Xcc").arg(format!("-I{}", dir.display()));
}

/// SwiftPM `systemLibrary(path: "generated")` and local `FFI/` maps, plus file-system dependency
/// roots listed in `.build/workspace-state.json` (their `generated/` trees).
fn append_package_clang_module_flags(cmd: &mut Command, pkg: &Path) {
    let generated_dir = pkg.join("generated");
    if generated_dir.is_dir() {
        append_modulemap_clang_flags(cmd, &generated_dir, true);
    }
    let ffi_dir = pkg.join("FFI");
    if ffi_dir.is_dir() {
        append_modulemap_clang_flags(cmd, &ffi_dir, false);
    }
    append_workspace_dependency_generated_clang(cmd, pkg);
}

fn append_workspace_dependency_generated_clang(cmd: &mut Command, pkg: &Path) {
    let ws = pkg.join(".build").join("workspace-state.json");
    let Ok(raw) = fs::read_to_string(&ws) else {
        return;
    };
    let Ok(v) = serde_json::from_str::<JsonValue>(&raw) else {
        return;
    };
    let Some(deps) = v
        .get("object")
        .and_then(|o| o.get("dependencies"))
        .and_then(|d| d.as_array())
    else {
        return;
    };
    let mut seen_generated: HashSet<PathBuf> = HashSet::new();
    for dep in deps {
        let Some(st) = dep.get("state") else {
            continue;
        };
        let Some(path_s) = st.get("path").and_then(|p| p.as_str()) else {
            continue;
        };
        let root = Path::new(path_s);
        for sub in [root.join("generated"), root.join("Generated")] {
            if sub.is_dir() {
                let key = fs::canonicalize(&sub).unwrap_or_else(|_| sub.clone());
                if seen_generated.insert(key) {
                    append_modulemap_clang_flags(cmd, &sub, true);
                }
            }
        }
    }
}

/// Closest ancestor containing `Package.swift` (used for optional `swift build` prep).
fn nearest_package_root(path: &Path) -> Option<PathBuf> {
    let mut dir = if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent()?.to_path_buf()
    };
    loop {
        if dir.join("Package.swift").exists() {
            return Some(dir);
        }
        dir = dir.parent()?.to_path_buf();
    }
}

/// Package root for `swift build` prep and Clang flags: prefer a manifest that actually owns the
/// Swift file, else fall back to any ancestor `Package.swift` (nested examples without a manifest).
fn package_hint_for_swift_tooling(path: &Path) -> Option<PathBuf> {
    package_root_containing_swift_file(path).or_else(|| nearest_package_root(path))
}

fn swift_build_prep(pkg: &Path) -> Result<(), SilEmitError> {
    let output = Command::new("swift")
        .arg("build")
        .current_dir(pkg)
        .output()
        .map_err(|e| SilEmitError::Msg(format!("swift build spawn failed: {e}")))?;
    if !output.status.success() {
        return Err(SilEmitError::Msg(format!(
            "swift build failed ({}) in {}\nstderr:\n{}",
            output.status,
            pkg.display(),
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    Ok(())
}

fn collect_modules_dirs(depth_left: u32, dir: &Path, out: &mut Vec<PathBuf>) {
    if depth_left == 0 {
        return;
    }
    let Ok(rd) = fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let p = entry.path();
        if !p.is_dir() {
            continue;
        }
        if p.file_name().and_then(|s| s.to_str()) == Some("Modules") {
            out.push(p);
            continue;
        }
        collect_modules_dirs(depth_left.saturating_sub(1), &p, out);
    }
}

fn collect_swiftmodule_parent_dirs(depth_left: u32, dir: &Path, out: &mut Vec<PathBuf>) {
    if depth_left == 0 {
        return;
    }
    let Ok(rd) = fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let p = entry.path();
        if p.is_dir() {
            if p.extension().and_then(|x| x.to_str()) == Some("swiftmodule") {
                if let Some(par) = p.parent() {
                    out.push(par.to_path_buf());
                }
            } else {
                collect_swiftmodule_parent_dirs(depth_left.saturating_sub(1), &p, out);
            }
        }
    }
}

fn module_import_paths_after_build(pkg: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let build = pkg.join(".build");
    if build.is_dir() {
        collect_modules_dirs(18, &build, &mut out);
        collect_swiftmodule_parent_dirs(26, &build, &mut out);
    }
    out.sort();
    out.dedup();
    out
}

fn load_combined_sources(inputs: &[PathBuf]) -> Result<String, SilEmitError> {
    let mut parts = Vec::with_capacity(inputs.len());
    for p in inputs {
        parts.push(fs::read_to_string(p)?);
    }
    Ok(parts.join("\n\n"))
}

/// Same Swift source set [`emit_textual_sil`] uses for `path` (package `Sources/` + `Generated/`, `Sources/` siblings without `Package.swift`, or a single file).
pub fn combined_swift_sources_for_path(path: &Path) -> Result<String, SilEmitError> {
    let inputs = resolve_swift_inputs(path)?;
    load_combined_sources(&inputs)
}

/// Same flag ordering idea as [`run_swiftc_emit_sil`]: generated Clang flags from package, then **`-I`**, then extra swiftc args, then primaries.
fn run_swiftc_typecheck(
    inputs: &[PathBuf],
    module_includes: &[PathBuf],
    package_root_for_clang: Option<&Path>,
) -> bool {
    if inputs.is_empty() {
        return false;
    }
    let swiftc_bin = swiftc_executable();
    let mut cmd = Command::new(&swiftc_bin);
    cmd.arg("-typecheck");
    #[cfg(target_os = "macos")]
    if let Some(sdk) = macos_sdk_path() {
        cmd.arg("-sdk").arg(sdk);
    }
    if let Ok(triple) = std::env::var("IN_SWIFT_TARGET")
        && !triple.is_empty()
    {
        cmd.arg("-target").arg(triple);
    }
    if let Some(pkg) = package_root_for_clang {
        append_package_clang_module_flags(&mut cmd, pkg);
    }
    for inc in module_includes {
        cmd.arg("-I").arg(inc);
    }
    append_extra_swiftc_args(&mut cmd);
    for p in inputs {
        cmd.arg(p);
    }
    cmd.stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn run_swiftc_typecheck_partitioned(
    inputs: &[PathBuf],
    module_includes: &[PathBuf],
    package_root_for_clang: Option<&Path>,
) -> bool {
    if inputs.is_empty() {
        return false;
    }
    let (singles, multis) = partition_inputs_by_basename(inputs);
    if multis.is_empty() {
        return run_swiftc_typecheck(inputs, module_includes, package_root_for_clang);
    }
    let ok_singles = singles.is_empty()
        || run_swiftc_typecheck(&singles, module_includes, package_root_for_clang);
    let ok_multis = multis.iter().all(|group| {
        group.iter().all(|p| {
            run_swiftc_typecheck(std::slice::from_ref(p), module_includes, package_root_for_clang)
        })
    });
    ok_singles && ok_multis
}

/// First **`swiftc -typecheck`** (with generated flags from package root if any); on failure run **`swift build`** then retry with **`-I`** from `.build` (same strategy as [`emit_textual_sil`]).
fn run_swiftc_typecheck_with_package_retry(path: &Path, inputs: &[PathBuf]) -> bool {
    let pkg_hint = package_hint_for_swift_tooling(path);
    let pkg_clang = pkg_hint.as_deref();
    let warm_includes: Vec<PathBuf> = pkg_hint
        .as_ref()
        .map(|p| module_import_paths_after_build(p))
        .unwrap_or_default();
    let first_includes: &[PathBuf] = if warm_includes.is_empty() {
        &[]
    } else {
        &warm_includes
    };
    if run_swiftc_typecheck_partitioned(inputs, first_includes, pkg_clang) {
        return true;
    }
    let Some(pkg) = pkg_hint else {
        return false;
    };
    if swift_build_prep(&pkg).is_err() {
        return false;
    }
    let includes = module_import_paths_after_build(&pkg);
    run_swiftc_typecheck_partitioned(inputs, &includes, Some(pkg.as_path()))
}

/// Hot reload / tooling gate: same **`IN_NATIVE_SWIFT_SIL`** policy as SIL emit (`subset` first on try/only, else **`swiftc -typecheck`**).
pub fn compile_check_swift_path_with_mode(path: &Path, mode: NativeSwiftSilMode) -> bool {
    let Ok(inputs) = resolve_swift_inputs(path) else {
        return false;
    };
    let Ok(combined) = load_combined_sources(&inputs) else {
        return false;
    };
    match mode {
        NativeSwiftSilMode::Only => crate::native_swift_sil::swift_subset_typecheck_ok(&combined),
        NativeSwiftSilMode::Try => {
            if crate::native_swift_sil::swift_subset_typecheck_ok(&combined) {
                true
            } else {
                run_swiftc_typecheck_with_package_retry(path, &inputs)
            }
        }
        NativeSwiftSilMode::Off => run_swiftc_typecheck_with_package_retry(path, &inputs),
    }
}

/// Like [`compile_check_swift_path_with_mode`] using **`IN_NATIVE_SWIFT_SIL`** from the environment.
pub fn compile_check_swift_path(path: &Path) -> bool {
    compile_check_swift_path_with_mode(path, native_swift_sil_mode_from_env())
}

fn run_swiftc_emit_sil_batch(
    inputs: &[PathBuf],
    module_id: &str,
    module_includes: &[PathBuf],
    package_root_for_clang: Option<&Path>,
) -> Result<String, SilEmitError> {
    let seq = SWIFT_SIL_TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let out_file = std::env::temp_dir().join(format!(
        "inauguration-sil-{}-{}.sil",
        std::process::id(),
        seq
    ));

    let swiftc_bin = swiftc_executable();

    let mut cmd = Command::new(&swiftc_bin);
    cmd.arg("-emit-sil");
    cmd.arg("-O");
    cmd.arg("-suppress-warnings");
    cmd.arg("-module-name").arg(module_id);
    // Multiple primary files require WMO for a single `-o` SIL output.
    if inputs.len() > 1 {
        cmd.arg("-whole-module-optimization");
    }
    cmd.arg("-o").arg(&out_file);

    #[cfg(target_os = "macos")]
    if let Some(sdk) = macos_sdk_path() {
        cmd.arg("-sdk").arg(sdk);
    }

    if let Ok(triple) = std::env::var("IN_SWIFT_TARGET")
        && !triple.is_empty()
    {
        cmd.arg("-target").arg(triple);
    }

    if let Some(pkg) = package_root_for_clang {
        append_package_clang_module_flags(&mut cmd, pkg);
    }

    for inc in module_includes {
        cmd.arg("-I").arg(inc);
    }

    append_extra_swiftc_args(&mut cmd);
    for p in inputs {
        cmd.arg(p);
    }

    let output = cmd.output().map_err(|e| {
        SilEmitError::Msg(format!(
            "failed to spawn {} (set IN_SWIFTC or install a Swift toolchain): {e}",
            swiftc_bin.to_string_lossy()
        ))
    })?;

    if !output.status.success() {
        let _ = fs::remove_file(&out_file);
        return Err(SilEmitError::Msg(format!(
            "{} -emit-sil failed ({}) for {} inputs\nstderr:\n{}",
            swiftc_bin.to_string_lossy(),
            output.status,
            inputs.len(),
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    let sil = fs::read_to_string(&out_file).map_err(|e| {
        SilEmitError::Msg(format!(
            "read emitted SIL: {e}; stderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        ))
    })?;
    let _ = fs::remove_file(&out_file);

    if sil.trim().is_empty() {
        return Err(SilEmitError::Msg(
            "swiftc produced empty SIL (unexpected)".into(),
        ));
    }

    Ok(sil)
}

fn run_swiftc_emit_sil(
    inputs: &[PathBuf],
    module_id: &str,
    module_includes: &[PathBuf],
    package_root_for_clang: Option<&Path>,
) -> Result<String, SilEmitError> {
    if inputs.is_empty() {
        return Err(SilEmitError::Msg(
            "no Swift inputs for swiftc -emit-sil".into(),
        ));
    }
    let (singles, multis) = partition_inputs_by_basename(inputs);
    if multis.is_empty() {
        return run_swiftc_emit_sil_batch(
            inputs,
            module_id,
            module_includes,
            package_root_for_clang,
        );
    }
    let mut fragments: Vec<String> = Vec::new();
    if !singles.is_empty() {
        fragments.push(run_swiftc_emit_sil_batch(
            &singles,
            module_id,
            module_includes,
            package_root_for_clang,
        )?);
    }
    for group in multis {
        for p in group {
            fragments.push(run_swiftc_emit_sil_batch(
                std::slice::from_ref(&p),
                module_id,
                module_includes,
                package_root_for_clang,
            )?);
        }
    }
    Ok(fragments.join("\n\n// inauguration: sil fragment (basename-partitioned emit)\n\n"))
}

/// Emit canonical textual SIL for the Swift inputs implied by `path` / `module_id`.
///
/// Input resolution: under a `Package.swift` root, collects `Sources/**` plus `Generated/**` when
/// present (SwiftPM targets often list both). Without a package, all `*.swift` siblings under a
/// `Sources/` directory compile together (examples with no `Package.swift`). Otherwise uses
/// `Sources/` or `swift/Sources/` under the nearest package root.
///
/// Clang / C module maps: `<pkg>/generated` (skips default `module.modulemap` when other maps
/// exist, UniFFI-style), `<pkg>/FFI` (includes `module.modulemap` for `systemLibrary` targets), and
/// each file-system dependency’s `generated/` / `Generated/` from `.build/workspace-state.json`.
///
/// When **`IN_NATIVE_SWIFT_SIL`** is **`try`** or **`only`**, attempts in-tree subset SIL first.
///
/// Tries `swiftc -emit-sil` directly (partitioning primaries that share a basename). When a
/// package root is known, passes **`-I`** paths from an existing **`.build/**/Modules`** tree on the
/// first attempt so a prior **`swift build`** (e.g. from CI or a benchmark warm-up) is honored.
/// On failure, runs **`swift build`** in that package, then retries with refreshed **`-I`** paths.
pub fn emit_textual_sil(path: &Path, module_id: &str) -> Result<String, SilEmitError> {
    let inputs = resolve_swift_inputs(path)?;
    let combined = load_combined_sources(&inputs)?;

    match native_swift_sil_mode_from_env() {
        NativeSwiftSilMode::Only => {
            return crate::native_swift_sil::emit_in_tree_sil_or_diagnose(&combined, module_id)
                .map_err(SilEmitError::Msg);
        }
        NativeSwiftSilMode::Try => {
            if let Some(sil) = crate::native_swift_sil::try_emit_in_tree_sil(&combined, module_id) {
                return Ok(sil);
            }
        }
        NativeSwiftSilMode::Off => {}
    }

    let pkg_hint = package_hint_for_swift_tooling(path);
    let pkg_clang = pkg_hint.as_deref();
    let warm_includes: Vec<PathBuf> = pkg_hint
        .as_ref()
        .map(|p| module_import_paths_after_build(p))
        .unwrap_or_default();
    let first_includes: &[PathBuf] = if warm_includes.is_empty() {
        &[]
    } else {
        &warm_includes
    };
    match run_swiftc_emit_sil(&inputs, module_id, first_includes, pkg_clang) {
        Ok(s) => Ok(s),
        Err(first_err) => {
            let Some(pkg) = pkg_hint else {
                return Err(first_err);
            };
            swift_build_prep(&pkg)?;
            let includes = module_import_paths_after_build(&pkg);
            run_swiftc_emit_sil(&inputs, module_id, &includes, Some(pkg.as_path())).map_err(|second_err| {
                SilEmitError::Msg(format!(
                    "{second_err}\nEarlier attempt (before swift build prep in {}):\n{first_err}",
                    pkg.display()
                ))
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn resolve_single_swift_without_package() {
        let dir = std::env::temp_dir().join(format!("in-sil-test-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let f = dir.join("Lonely.swift");
        let mut file = fs::File::create(&f).unwrap();
        writeln!(file, "public func f() {{}}").unwrap();
        let v = resolve_swift_inputs(&f).unwrap();
        assert_eq!(v.len(), 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn partition_inputs_splits_duplicate_basenames() {
        let a = PathBuf::from("/proj/Sources/A/Foo.swift");
        let b = PathBuf::from("/proj/Sources/B/Foo.swift");
        let c = PathBuf::from("/proj/Sources/Bar.swift");
        let (singles, multis) = partition_inputs_by_basename(&[a.clone(), b.clone(), c.clone()]);
        assert_eq!(singles.len(), 1);
        assert_eq!(singles[0], c);
        assert_eq!(multis.len(), 1);
        assert_eq!(multis[0].len(), 2);
        assert!(multis[0].contains(&a));
        assert!(multis[0].contains(&b));
    }

    #[test]
    fn resolve_example_sources_dir_collects_siblings() {
        let base = std::env::temp_dir().join(format!("in-sil-ex-{}", std::process::id()));
        let sources = base.join("Sources");
        let _ = fs::create_dir_all(&sources);
        let a = sources.join("App.swift");
        let b = sources.join("More.swift");
        writeln!(fs::File::create(&a).unwrap(), "import Foundation").unwrap();
        writeln!(fs::File::create(&b).unwrap(), "import Foundation").unwrap();
        let v = resolve_swift_inputs(&a).unwrap();
        assert_eq!(v.len(), 2);
        let ca = fs::canonicalize(&a).unwrap();
        let cb = fs::canonicalize(&b).unwrap();
        assert!(v.contains(&ca));
        assert!(v.contains(&cb));
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn native_subset_sample_emits_sil_without_swiftc() {
        let sample = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../apps/native-subset-sample/App.swift");
        if !sample.exists() {
            return;
        }
        let src = fs::read_to_string(&sample).unwrap();
        let sil = crate::native_swift_sil::emit_in_tree_sil_or_diagnose(&src, "App").unwrap();
        assert!(sil.contains("sil @main"));
        assert!(sil.contains("function_ref @helper"));
    }

    #[test]
    fn compile_check_only_accepts_subset_with_main() {
        let dir = std::env::temp_dir().join(format!("in-compile-only-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let ok_path = dir.join("ok.swift");
        let mut f = fs::File::create(&ok_path).unwrap();
        writeln!(f, "struct User").unwrap();
        writeln!(f, "func main() -> Void").unwrap();
        assert!(compile_check_swift_path_with_mode(
            &ok_path,
            NativeSwiftSilMode::Only
        ));

        let bad = dir.join("bad.swift");
        let mut f2 = fs::File::create(&bad).unwrap();
        writeln!(f2, "struct X {{}}").unwrap();
        assert!(!compile_check_swift_path_with_mode(
            &bad,
            NativeSwiftSilMode::Only
        ));

        let _ = fs::remove_dir_all(&dir);
    }
}
