use std::fs;
use std::path::Path;

use toml_edit::DocumentMut;

#[test]
fn cargo_dist_config_builds_the_server_installer_target() {
    let config = read_toml("Cargo.toml");
    let dist = config["workspace"]["metadata"]["dist"]
        .as_table()
        .expect("Cargo.toml must contain [workspace.metadata.dist]");

    assert_eq!(dist["cargo-dist-version"].as_str(), Some("0.30.2"));
    assert_eq!(dist["ci"].as_str(), Some("github"));
    assert_eq!(string_array(dist["allow-dirty"].as_array()), vec!["ci"]);
    assert_eq!(string_array(dist["installers"].as_array()), vec!["shell"]);
    assert_eq!(
        string_array(dist["targets"].as_array()),
        vec!["x86_64-unknown-linux-musl"]
    );
    assert_eq!(dist["install-path"].as_str(), Some("CARGO_HOME"));
    assert_eq!(dist["install-updater"].as_bool(), Some(false));
}

#[test]
fn release_workflow_uses_cargo_dist_artifacts() {
    let workflow = fs::read_to_string(".github/workflows/release_please.yml")
        .expect("release workflow should be readable");

    assert!(workflow.contains("CARGO_DIST_VERSION: 0.30.2"));
    assert!(workflow.contains("dist build"));
    assert!(workflow.contains("dist print-upload-files-from-manifest"));
    assert!(workflow.contains("dist-upload-files.txt"));
    assert!(!workflow.contains("Package release tarball"));
    assert!(!workflow.contains("dist/agent-harness-${TARGET}.tar.gz"));
}

fn read_toml(path: impl AsRef<Path>) -> DocumentMut {
    fs::read_to_string(path)
        .expect("toml file should be readable")
        .parse::<DocumentMut>()
        .expect("toml file should parse")
}

fn string_array(array: Option<&toml_edit::Array>) -> Vec<&str> {
    array
        .expect("value should be an array")
        .iter()
        .map(|value| value.as_str().expect("array item should be a string"))
        .collect()
}
