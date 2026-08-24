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
    let config = read_toml("Cargo.toml");
    let version = config["workspace"]["metadata"]["dist"]["cargo-dist-version"]
        .as_str()
        .expect("cargo-dist-version should be a string");
    let workflow = fs::read_to_string(".github/workflows/release_please.yml")
        .expect("release workflow should be readable");

    assert!(workflow.contains(&format!("CARGO_DIST_VERSION: {version}")));
    assert!(workflow.contains("dist build"));
    assert!(workflow.contains("dist print-upload-files-from-manifest"));
    assert!(workflow.contains("gh release upload"));
}

#[test]
fn release_please_bumps_breaking_pre_major_releases_by_minor() {
    let config = read_json("release-please-config.json");

    assert_eq!(
        config["packages"]["."]["bump-minor-pre-major"].as_bool(),
        Some(true)
    );
}

fn read_toml(path: impl AsRef<Path>) -> DocumentMut {
    fs::read_to_string(path)
        .expect("toml file should be readable")
        .parse::<DocumentMut>()
        .expect("toml file should parse")
}

fn read_json(path: impl AsRef<Path>) -> serde_json::Value {
    serde_json::from_str(&fs::read_to_string(path).expect("json file should be readable"))
        .expect("json file should parse")
}

fn string_array(array: Option<&toml_edit::Array>) -> Vec<&str> {
    array
        .expect("value should be an array")
        .iter()
        .map(|value| value.as_str().expect("array item should be a string"))
        .collect()
}
