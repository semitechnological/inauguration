//! Workspace-only hooks: probe `equilibrium-ffi` against `generate_models.v`, emit rerun directives.

fn main() {
    let manifest_dir =
        std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default());
    let schema = manifest_dir.join("../../shared/protocol/events.schema.json");
    let gen_v = manifest_dir.join("../../shared/protocol/generate_models.v");

    if schema.exists() {
        println!("cargo:rerun-if-changed={}", schema.display());
    }
    if gen_v.exists() {
        println!("cargo:rerun-if-changed={}", gen_v.display());
    }

    let Some(out_dir) = std::env::var_os("OUT_DIR") else {
        return;
    };
    let out_dir = std::path::PathBuf::from(out_dir).join("equilibrium_probe");
    if gen_v.exists() {
        let _ = std::fs::create_dir_all(&out_dir);
        match equilibrium_ffi::compile_to_c(&gen_v, &out_dir) {
            Ok(res) => {
                println!(
                    "cargo:warning=equilibrium-ffi: compiled protocol generator {} -> {}",
                    gen_v.display(),
                    res.output_path.display()
                );
            }
            Err(e) => println!(
                "cargo:warning=equilibrium-ffi skipped (install `v` toolchain for C backend): {e}"
            ),
        }
    }
}
