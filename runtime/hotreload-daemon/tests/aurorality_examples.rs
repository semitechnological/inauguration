use hotreload_daemon::{plan_patch, symbols_for_path};
use std::path::PathBuf;

#[test]
fn classifies_aurorality_example_files() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let aurorality = repo_root.join("../../../aurorality/examples");
    assert!(aurorality.exists(), "aurorality examples missing: {}", aurorality.display());

    let app = aurorality.join("counter/Sources/App.swift");
    let basic = aurorality.join("basic/Sources/App.swift");
    let hyperchat = aurorality.join("hyperchat/Sources/HyperChatRootView.swift");

    for path in [&app, &basic, &hyperchat] {
        assert!(path.exists(), "missing example file: {}", path.display());
    }

    let app_patch = plan_patch(&app.to_string_lossy(), &symbols_for_path(&app.to_string_lossy()));
    assert!(!app_patch.compatible);

    let root_patch = plan_patch(
        &hyperchat.to_string_lossy(),
        &symbols_for_path(&hyperchat.to_string_lossy()),
    );
    assert!(!root_patch.target.is_empty());
}
