fn get_schema_path(manifest_dir: &std::path::Path) -> std::path::PathBuf {
    manifest_dir.join("../../shared/protocol/events.schema.json")
}

fn main() {
    let manifest_dir =
        std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default());
    let schema = get_schema_path(&manifest_dir);
    if schema.exists() {
        println!("cargo:rerun-if-changed={}", schema.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    #[test]
    fn test_get_schema_path() {
        let manifest_dir = Path::new("/tmp/foo");
        let expected = PathBuf::from("/tmp/foo/../../shared/protocol/events.schema.json");
        assert_eq!(get_schema_path(manifest_dir), expected);
    }
}
