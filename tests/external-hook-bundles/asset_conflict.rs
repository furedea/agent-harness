use super::{
    remove_dir, repo_root, run_harness_output, test_root, write_external_hook_asset_bundle,
};

#[test]
fn install_rejects_two_bundles_that_map_to_one_installed_path() {
    let root = test_root("external-hook-assets-conflict");
    let bundle = write_external_hook_asset_bundle(&root);
    let prefix = root.join("home");
    let extra_hook = format!("herdr={}", bundle.display());

    let output = run_harness_output([
        "install",
        "--source",
        repo_root().to_str().unwrap(),
        "--extra-hook",
        &extra_hook,
        "--extra-hook",
        &extra_hook,
        "--prefix",
        prefix.to_str().unwrap(),
    ]);

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("conflicts with"));

    remove_dir(root);
}
