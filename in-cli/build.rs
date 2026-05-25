use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo::rustc-check-cfg=cfg(has_v_native)");

    let manifest_dir =
        std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default());

    let schema = manifest_dir.join("../../shared/protocol/events.schema.json");
    if schema.exists() {
        println!("cargo:rerun-if-changed={}", schema.display());
    }

    let v_native_dir = manifest_dir.join("v-native");
    println!("cargo:rerun-if-changed={}", v_native_dir.display());

    let v_ok = match which::which("v") {
        Ok(v_path) => {
            println!("cargo:warning=found v compiler at {}", v_path.display());
            true
        }
        Err(_) => {
            println!("cargo:warning=v compiler not found; skipping V native build");
            false
        }
    };

    if !v_ok {
        return;
    }

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap_or_default());

    let v_files: Vec<PathBuf> = vec![
        v_native_dir.join("inc_compiler.v"),
        v_native_dir.join("inrt.v"),
        v_native_dir.join("parallel.v"),
        v_native_dir.join("orchestration.v"),
        v_native_dir.join("bench.v"),
    ];

    let mut c_objects = Vec::new();

    for v_file in &v_files {
        if !v_file.exists() {
            continue;
        }

        let stem = v_file.file_stem().and_then(|s| s.to_str()).unwrap_or("out");
        let c_file = out_dir.join(format!("{stem}.c"));

        let status = Command::new("v")
            .args([
                "-shared",
                "-gc", "none",
                "-prod",
                "-backend", "c",
                "-o", c_file.to_str().unwrap_or("out.c"),
                v_file.to_str().unwrap_or(""),
            ])
            .status();

        match status {
            Ok(s) if s.success() => {
                println!("cargo:warning=compiled {} to C", v_file.display());
            }
            Ok(s) => {
                println!("cargo:warning=v failed for {} with exit {:?}", v_file.display(), s.code());
                return;
            }
            Err(e) => {
                println!("cargo:warning=v invocation failed for {}: {e}", v_file.display());
                return;
            }
        }

        if !c_file.exists() {
            println!("cargo:warning=expected C output {} not found", c_file.display());
            return;
        }

        let obj_file = out_dir.join(format!("{stem}.o"));
        let cc_status = Command::new("cc")
            .args([
                "-c",
                c_file.to_str().unwrap_or(""),
                "-o", obj_file.to_str().unwrap_or(""),
                "-O2",
            ])
            .status();

        match cc_status {
            Ok(s) if s.success() => {
                println!("cargo:warning=compiled {} to object", c_file.display());
                c_objects.push(obj_file);
            }
            Ok(s) => {
                println!("cargo:warning=cc failed for {} with exit {:?}", c_file.display(), s.code());
                return;
            }
            Err(e) => {
                println!("cargo:warning=cc invocation failed: {e}");
                return;
            }
        }
    }

    if c_objects.is_empty() {
        return;
    }

    let lib_path = out_dir.join("libinc_compiler.a");

    let mut ar = Command::new("ar");
    ar.arg("crs").arg(&lib_path);
    for obj in &c_objects {
        ar.arg(obj);
    }
    let ar_status = ar.status();
    match ar_status {
        Ok(s) if s.success() => {}
        _ => {
            println!("cargo:warning=ar failed to create static library");
            return;
        }
    }

    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=static=inc_compiler");
    println!("cargo:rustc-cfg=has_v_native");
    println!("cargo:warning=V native build complete -> {}", lib_path.display());
}
