use serde_json::Value;

use super::{complete_source_root, read_json, remove_dir, run_harness, test_root};

#[test]
fn dsh_manifest_generation_defaults_to_empty_hooks() {
    let root = test_root("dsh-manifest");
    let output = root.join("hooks.json");
    run_harness(["generate-dsh-hooks", "--output", output.to_str().unwrap()]);
    let hooks: Value = serde_json::from_str(&std::fs::read_to_string(output).unwrap()).unwrap();
    assert_eq!(hooks, serde_json::json!({"hooks": {}}));
    remove_dir(root);
}

#[test]
fn dsh_install_places_manifest_bundle_skills_and_instructions() {
    let root = test_root("dsh-install");
    run_harness([
        "install",
        "--source",
        complete_source_root().to_str().unwrap(),
        "--prefix",
        root.to_str().unwrap(),
    ]);
    for file in [
        ".dsh/hooks.json",
        ".dsh/hooks/hook_adapter.py",
        ".dsh/bridge/package.json",
        ".dsh/bridge/hook_bridge.ts",
        ".dsh/AGENTS.md",
    ] {
        assert!(root.join(file).is_file(), "missing {file}");
    }
    assert!(root.join(".dsh/skills").is_dir());
    remove_dir(root);
}

#[test]
fn dsh_sync_preserves_user_profile_and_is_idempotent() {
    let root = test_root("dsh-sync");
    let target = root.join("package.json");
    let bundle = root.join("bridge");
    std::fs::create_dir_all(&bundle).unwrap();
    std::fs::write(&target, r#"{"private":true,"dependencies":{"custom":"1"},"dsh":{"profile":{"bundles":["custom"],"other":true}}}"#).unwrap();
    for _ in 0..2 {
        run_harness([
            "sync-dsh-config",
            "--source",
            bundle.to_str().unwrap(),
            "--target",
            target.to_str().unwrap(),
        ]);
    }
    let config = read_json(&target);
    assert_eq!(config["private"], true);
    assert_eq!(config["dependencies"]["custom"], "1");
    assert_eq!(config["dsh"]["profile"]["other"], true);
    assert_eq!(
        config["dsh"]["profile"]["bundles"],
        serde_json::json!(["custom", "agent-harness-dsh-hooks"])
    );
    assert_eq!(
        config["dependencies"]["agent-harness-dsh-hooks"],
        format!("link:{}", bundle.canonicalize().unwrap().display())
    );
    remove_dir(root);
}

#[test]
fn dsh_sync_skips_uninitialized_profile() {
    let root = test_root("dsh-uninitialized");
    let target = root.join("profiles/custom/package.json");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_agent-harness"))
        .args([
            "sync-dsh-config",
            "--source",
            root.to_str().unwrap(),
            "--target",
            target.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(!target.exists());
    assert!(String::from_utf8_lossy(&output.stderr).contains("run dsh once"));
    remove_dir(root);
}

#[test]
fn dsh_custom_profile_is_protected() {
    let root = test_root("dsh-custom-profile");
    run_harness([
        "install",
        "--prefix",
        root.to_str().unwrap(),
        "--dsh-profile",
        "custom",
    ]);
    let policy =
        std::fs::read_to_string(root.join(".claude/hooks/rules/protected_paths.json")).unwrap();
    let codex = std::fs::read_to_string(root.join(".codex/config.toml")).unwrap();
    let claude = std::fs::read_to_string(root.join(".claude/settings.json")).unwrap();
    for content in [policy, codex, claude] {
        assert!(content.contains(".dsh/profiles/custom/package.json"));
        assert!(!content.contains(".dsh/profiles/web/package.json"));
    }
    remove_dir(root);
}
