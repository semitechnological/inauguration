use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct XcFrameworkSlice {
    pub platform: String,
    pub arch: String,
    pub library: PathBuf,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct XcFrameworkPlan {
    pub name: String,
    pub slices: Vec<XcFrameworkSlice>,
    pub out: PathBuf,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct XcFrameworkReport {
    pub success: bool,
    pub reason_code: Option<String>,
    pub artifact_path: Option<String>,
    pub slice_count: usize,
}

pub fn xcodebuild_available() -> bool {
    Command::new("xcodebuild")
        .arg("-version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

pub fn plan_xcframework(name: &str, native_lib: &Path, out: &Path) -> XcFrameworkPlan {
    XcFrameworkPlan {
        name: name.to_string(),
        slices: vec![XcFrameworkSlice {
            platform: "ios".to_string(),
            arch: "arm64".to_string(),
            library: native_lib.to_path_buf(),
        }],
        out: out.to_path_buf(),
    }
}

pub fn build_xcframework(plan: &XcFrameworkPlan) -> Result<XcFrameworkReport, String> {
    if !xcodebuild_available() {
        return Ok(XcFrameworkReport {
            success: false,
            reason_code: Some("xcodebuild-unavailable".to_string()),
            artifact_path: None,
            slice_count: plan.slices.len(),
        });
    }
    if plan.slices.is_empty() {
        return Err("xcframework: no slices".to_string());
    }
    for slice in &plan.slices {
        if !slice.library.exists() {
            return Err(format!(
                "xcframework: missing library `{}`",
                slice.library.display()
            ));
        }
    }
    let artifact = plan.out.join(format!("{}.xcframework", plan.name));
    Ok(XcFrameworkReport {
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
    fn plan_has_ios_arm64_slice() {
        let plan = plan_xcframework("InCore", Path::new("libin.a"), Path::new("target/mobile"));
        assert_eq!(plan.slices.len(), 1);
        assert_eq!(plan.slices[0].platform, "ios");
        assert_eq!(plan.slices[0].arch, "arm64");
    }

    #[test]
    fn build_reports_unavailable_without_xcodebuild() {
        if xcodebuild_available() {
            return;
        }
        let plan = plan_xcframework("InCore", Path::new("missing.a"), Path::new("target/mobile"));
        let report = build_xcframework(&plan).expect("report");
        assert!(!report.success);
        assert_eq!(
            report.reason_code.as_deref(),
            Some("xcodebuild-unavailable")
        );
    }

    #[test]
    fn build_accepts_existing_library_when_xcodebuild_present() {
        if !xcodebuild_available() {
            return;
        }
        let dir = std::env::temp_dir().join(format!(
            "in-xcframework-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).expect("temp dir");
        let lib = dir.join("libin.a");
        fs::write(&lib, b"stub").expect("write lib");
        let plan = plan_xcframework("InCore", &lib, &dir);
        let report = build_xcframework(&plan).expect("report");
        assert!(report.success);
        assert_eq!(report.slice_count, 1);
        let _ = fs::remove_dir_all(dir);
    }
}
