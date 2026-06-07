use std::fs;
use std::path::Path;

pub const RAW_ENTRY_OFFSET: u32 = 0;

pub fn write_raw_binary(code: &[u8], out_path: &Path) -> Result<(), String> {
    if code.is_empty() {
        return Err("raw: empty code section".to_string());
    }
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("raw: create parent `{}`: {err}", parent.display()))?;
    }
    fs::write(out_path, code).map_err(|err| format!("raw: write `{}`: {err}", out_path.display()))
}

pub fn freestanding_stub() -> Vec<u8> {
    vec![
        0x00, 0x00, 0x80, 0xD2,
        0xC0, 0x03, 0x5F, 0xD6,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn write_raw_binary_roundtrip() {
        let dir = std::env::temp_dir().join(format!(
            "in-raw-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).expect("temp dir");
        let out = dir.join("image.bin");
        let code = freestanding_stub();
        write_raw_binary(&code, &out).expect("write");
        let read = fs::read(&out).expect("read");
        assert_eq!(read, code);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn rejects_empty_payload() {
        let dir = std::env::temp_dir().join(format!(
            "in-raw-empty-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).expect("temp dir");
        let out = dir.join("empty.bin");
        let err = write_raw_binary(&[], &out).expect_err("empty");
        assert!(err.contains("empty code section"));
        let _ = fs::remove_dir_all(dir);
    }
}