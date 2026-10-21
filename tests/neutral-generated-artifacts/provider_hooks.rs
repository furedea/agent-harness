use super::{complete_source_root, read_json, remove_dir, run_harness, test_root};

#[test]
fn complete_source_generates_devin_hooks() {
    let root = test_root("devin-hooks");
    let config_path = root.join("config.json");

    run_harness([
        "generate-devin-hooks",
        "--source",
        complete_source_root().to_str().unwrap(),
        "--output",
        config_path.to_str().unwrap(),
    ]);

    let config = read_json(&config_path);
    assert_eq!(
        config["hooks"]["PreToolUse"][0]["hooks"][0]["command"].as_str(),
        Some("$HOME/.devin/hooks/hook_adapter.py shell forbidden"),
    );

    remove_dir(root);
}

#[test]
fn complete_source_generates_hermes_hooks() {
    let root = test_root("hermes-hooks");
    let manifest_path = root.join("hooks.json");

    run_harness([
        "generate-hermes-hooks",
        "--source",
        complete_source_root().to_str().unwrap(),
        "--output",
        manifest_path.to_str().unwrap(),
    ]);

    let manifest = read_json(&manifest_path);
    assert_eq!(
        manifest["hooks"]["pre_tool_call"][0]["hooks"][0]["command"].as_str(),
        Some("$HOME/.hermes/hooks/hook_adapter.py shell forbidden"),
    );

    remove_dir(root);
}

#[test]
fn complete_source_generates_pi_hooks() {
    let root = test_root("pi-hooks");
    let manifest_path = root.join("hooks.json");

    run_harness([
        "generate-pi-hooks",
        "--source",
        complete_source_root().to_str().unwrap(),
        "--output",
        manifest_path.to_str().unwrap(),
    ]);

    let manifest = read_json(&manifest_path);
    assert_eq!(
        manifest["hooks"]["tool_call"][0]["hooks"][0]["command"].as_str(),
        Some("$HOME/.pi/hooks/hook_adapter.py shell forbidden"),
    );

    remove_dir(root);
}
