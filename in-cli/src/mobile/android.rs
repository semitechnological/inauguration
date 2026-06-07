use serde::Serialize;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AarSlice {
    pub abi: String,
    pub shared_object: PathBuf,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AarPlan {
    pub package: String,
    pub version: String,
    pub slices: Vec<AarSlice>,
    pub out: PathBuf,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AarReport {
    pub success: bool,
    pub reason_code: Option<String>,
    pub artifact_path: Option<String>,
    pub slice_count: usize,
}

pub fn plan_aar(package: &str, native_lib: &Path, out: &Path) -> AarPlan {
    AarPlan {
        package: package.to_string(),
        version: "0.1.0".to_string(),
        slices: vec![AarSlice {
            abi: "arm64-v8a".to_string(),
            shared_object: native_lib.to_path_buf(),
        }],
        out: out.to_path_buf(),
    }
}

pub fn build_aar_skeleton(plan: &AarPlan) -> Result<AarReport, String> {
    if plan.slices.is_empty() {
        return Err("aar: no slices".to_string());
    }
    for slice in &plan.slices {
        if !slice.shared_object.exists() {
            return Err(format!(
                "aar: missing shared object `{}`",
                slice.shared_object.display()
            ));
        }
    }
    fs::create_dir_all(&plan.out).map_err(|err| format!("aar: create out dir: {err}"))?;
    let artifact = plan.out.join(format!(
        "{}-{}.aar",
        plan.package.replace('.', "_"),
        plan.version
    ));
    let mut file = fs::File::create(&artifact)
        .map_err(|err| format!("aar: create `{}`: {err}", artifact.display()))?;
    let manifest = format!(
        "<manifest package=\"{}\" version=\"{}\"></manifest>\n",
        plan.package, plan.version
    );
    file.write_all(manifest.as_bytes())
        .map_err(|err| format!("aar: write manifest: {err}"))?;
    Ok(AarReport {
        success: true,
        reason_code: None,
        artifact_path: Some(artifact.display().to_string()),
        slice_count: plan.slices.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn plan_has_arm64_slice() {
        let plan = plan_aar("dev.inauguration.core", Path::new("libin.so"), Path::new("target/mobile"));
        assert_eq!(plan.slices.len(), 1);
        assert_eq!(plan.slices[0].abi, "arm64-v8a");
    }

    #[test]
    fn build_writes_manifest_aar() {
        let dir = std::env::temp_dir().join(format!(
            "in-aar-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).expect("temp dir");
        let lib = dir.join("libin.so");
        fs::write(&lib, b"\x7fELF").expect("write lib");
        let plan = plan_aar("dev.inauguration.core", &lib, &dir);
        let report = build_aar_skeleton(&plan).expect("report");
        assert!(report.success);
        assert_eq!(report.slice_count, 1);
        let artifact = PathBuf::from(report.artifact_path.expect("artifact"));
        assert!(artifact.exists());
        let contents = fs::read_to_string(&artifact).expect("read aar");
        assert!(contents.contains("dev.inauguration.core"));
        let _ = fs::remove_dir_all(dir);
    }
}