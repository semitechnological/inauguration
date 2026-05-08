//! Rerun `cargo build` when the protocol schema changes (sources are regenerated via `protocol-gen`).

fn main() {
    let manifest_dir =
        std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default());
    let schema = manifest_dir.join("../../shared/protocol/events.schema.json");
    if schema.exists() {
        println!("cargo:rerun-if-changed={}", schema.display());
    }
}
