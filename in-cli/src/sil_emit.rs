//! Extract textual SIL from Swift sources via `swiftc` for native `in build`.
//!
//! This shells out to Apple's `swiftc` **only** as a SIL producer; orchestration, SIL passes,
//! and staging policy stay in `in`. Fully self-hosted SIL generation would replace this module.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SilEmitError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Msg(String),
}

/// Nearest ancestor directory containing `Package.swift` whose `Sources/**/*.swift` set is non-empty.
fn package_root_with_sources_for(path: &Path) -> Option<PathBuf> {
    let mut dir = path.parent()?;
    loop {
        if dir.join("Package.swift").exists() && collect_package_sources_flexible(dir).is_ok() {
            return Some(dir.to_path_buf());
        }
        dir = dir.parent()?;
    }
}

fn collect_sources_dir(pkg: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), SilEmitError> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let p = entry?.path();
        if p.is_dir() {
            collect_sources_dir(pkg, &p, out)?;
        } else if p.extension().and_then(|s| s.to_str()) == Some("swift") {
            out.push(p);
        }
    }
    Ok(())
}

/// All `*.swift` files under `<pkg>/Sources` (flat Package convention).
fn collect_package_sources(pkg: &Path) -> Result<Vec<PathBuf>, SilEmitError> {
    let sources = pkg.join("Sources");
    if !sources.is_dir() {
        return Err(SilEmitError::Msg(format!(
            "no Sources/ under {}",
            pkg.display()
        )));
    }
    let mut out = Vec::new();
    collect_sources_dir(pkg, &sources, &mut out)?;
    out.sort();
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
        collect_sources_dir(pkg, &swift_nested, &mut out)?;
        out.sort();
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
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
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
        if let Some(sibs) = collect_swift_siblings_in_sources_parent(path) {
            return Ok(sibs);
        }
        if let Some(pkg) = package_root_with_sources_for(path) {
            return collect_package_sources_flexible(&pkg);
        }
        return Ok(vec![path.to_path_buf()]);
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

/// Clang modules declared via SwiftPM `systemLibrary(path: "generated")` need explicit `-fmodule-map-file`.
fn append_generated_clang_flags(cmd: &mut Command, pkg: &Path) {
    let generated_dir = pkg.join("generated");
    if !generated_dir.is_dir() {
        return;
    }
    let Ok(rd) = fs::read_dir(&generated_dir) else {
        return;
    };
    let mut maps = Vec::new();
    for entry in rd.flatten() {
        let p = entry.path();
        if p.extension().and_then(|x| x.to_str()) == Some("modulemap") {
            // Skip umbrella maps that duplicate named `Foo.modulemap` entries (common in UniFFI layouts).
            if p.file_name().and_then(|s| s.to_str()) == Some("module.modulemap") {
                continue;
            }
            maps.push(p);
        }
    }
    maps.sort();
    for m in maps {
        cmd
            .arg("-Xcc")
            .arg(format!("-fmodule-map-file={}", m.display()));
    }
    cmd.arg("-Xcc").arg(format!("-I{}", generated_dir.display()));
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

fn run_swiftc_emit_sil(
    inputs: &[PathBuf],
    module_id: &str,
    module_includes: &[PathBuf],
    package_root_for_clang: Option<&Path>,
) -> Result<String, SilEmitError> {
    let out_file =
        std::env::temp_dir().join(format!("inauguration-sil-{}.sil", std::process::id()));

    let mut cmd = Command::new("swiftc");
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

    if let Ok(triple) = std::env::var("IN_SWIFT_TARGET") {
        if !triple.is_empty() {
            cmd.arg("-target").arg(triple);
        }
    }

    if let Some(pkg) = package_root_for_clang {
        append_generated_clang_flags(&mut cmd, pkg);
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
            "failed to spawn swiftc (install Swift toolchain): {e}"
        ))
    })?;

    if !output.status.success() {
        let _ = fs::remove_file(&out_file);
        return Err(SilEmitError::Msg(format!(
            "swiftc -emit-sil failed ({}) for {} inputs\nstderr:\n{}",
            output.status,
            inputs.len(),
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    let sil = fs::read_to_string(&out_file).map_err(|e| {
        let _ = fs::remove_file(&out_file);
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

/// Emit canonical textual SIL for the Swift inputs implied by `path` / `module_id`.
///
/// Input resolution: `.swift` files under a parent named `Sources/` compile together (SwiftPM-style
/// examples without a local `Package.swift`). Otherwise uses `Sources/` or `swift/Sources/` under the
/// nearest `Package.swift` root. Clang `systemLibrary` deps get `-Xcc -fmodule-map-file` / `-Xcc -I`
/// from `<pkg>/generated` when present (skips duplicate `module.modulemap`).
///
/// Tries `swiftc -emit-sil` directly; if that fails for package dependency reasons and we know a
/// `Package.swift` root, runs one **`swift build`** to populate `.build/**/Modules`, then retries
/// with `-I` for those directories (SwiftPM is dependency prep only, not the SIL producer path).
pub fn emit_textual_sil(path: &Path, module_id: &str) -> Result<String, SilEmitError> {
    let inputs = resolve_swift_inputs(path)?;
    let pkg_hint = nearest_package_root(path);
    let pkg_clang = pkg_hint.as_deref();
    match run_swiftc_emit_sil(&inputs, module_id, &[], pkg_clang) {
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
        assert!(v.contains(&a));
        assert!(v.contains(&b));
        let _ = fs::remove_dir_all(&base);
    }
}
