use super::{read_json, read_toml, remove_dir, run_harness, test_root};

#[test]
fn claude_settings_sync_expands_home_paths_only_after_opening_quotes() {
    let root = test_root("claude-sync-quoted-home");
    let home = root.join("custom home");
    let source_path = root.join("source.json");
    let target_path = root.join("target.json");
    let source = serde_json::json!({
        "sandbox": {
            "excludedCommands": [
                r#"cmd "foo"~/bar"#,
                r#"node "~/first" "~/second" *"#,
                r#"cmd \"~/bar"#,
                r#"node "foo\"bar" "~/second""#,
                r#"node "foo\\" "~/second""#
            ]
        }
    });
    std::fs::write(&source_path, serde_json::to_vec(&source).unwrap()).unwrap();

    super::run_harness_command(
        std::process::Command::new(env!("CARGO_BIN_EXE_agent-harness"))
            .env("HOME", &home)
            .args([
                "sync-claude-settings",
                "--source",
                source_path.to_str().unwrap(),
                "--target",
                target_path.to_str().unwrap(),
            ]),
    );

    assert_eq!(
        read_json(&target_path)["sandbox"]["excludedCommands"],
        serde_json::json!([
            r#"cmd "foo"~/bar"#,
            format!(r#"node "{0}/first" "{0}/second" *"#, home.display()),
            r#"cmd \"~/bar"#,
            format!(r#"node "foo\"bar" "{}/second""#, home.display()),
            format!(r#"node "foo\\" "{}/second""#, home.display())
        ])
    );

    remove_dir(root);
}

#[test]
fn hermes_config_sync_unions_plugins_enabled() {
    let root = test_root("hermes-sync");
    let source_path = root.join("managed.yaml");
    let target_path = root.join("config.yaml");

    std::fs::write(
        &source_path,
        "plugins:\n  enabled:\n    - agent-harness-hooks\n",
    )
    .unwrap();
    std::fs::write(
        &target_path,
        "model:\n  provider: openai-codex\nplugins:\n  enabled:\n    - moshi-hooks\n",
    )
    .unwrap();

    run_harness([
        "sync-hermes-config",
        "--source",
        source_path.to_str().unwrap(),
        "--target",
        target_path.to_str().unwrap(),
    ]);

    let content = std::fs::read_to_string(&target_path).unwrap();
    assert!(content.contains("model:"));
    assert!(content.contains("provider: openai-codex"));
    assert!(content.contains("- moshi-hooks"));
    assert!(content.contains("- agent-harness-hooks"));

    remove_dir(root);
}

#[test]
fn hermes_config_sync_dedupes_and_creates_target() {
    let root = test_root("hermes-sync-create");
    let source_path = root.join("managed.yaml");
    let target_path = root.join("nested/config.yaml");

    std::fs::write(
        &source_path,
        "plugins:\n  enabled:\n    - agent-harness-hooks\n",
    )
    .unwrap();

    run_harness([
        "sync-hermes-config",
        "--source",
        source_path.to_str().unwrap(),
        "--target",
        target_path.to_str().unwrap(),
    ]);

    let content = std::fs::read_to_string(&target_path).unwrap();
    assert!(content.contains("- agent-harness-hooks"));

    std::fs::write(
        &target_path,
        "plugins:\n  enabled:\n    - agent-harness-hooks\n",
    )
    .unwrap();
    run_harness([
        "sync-hermes-config",
        "--source",
        source_path.to_str().unwrap(),
        "--target",
        target_path.to_str().unwrap(),
    ]);
    let content = std::fs::read_to_string(&target_path).unwrap();
    assert_eq!(content.matches("agent-harness-hooks").count(), 1);

    remove_dir(root);
}

#[test]
fn devin_config_sync_preserves_user_owned_state() {
    let root = test_root("devin-sync");
    let source_path = root.join("source.json");
    let target_path = root.join("target.json");

    std::fs::write(
        &source_path,
        r#"{"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"generated"}]}]}}"#,
    )
    .unwrap();
    std::fs::write(
        &target_path,
        r#"{
  "version": 1,
  "devin": {"org_id": "org-123"},
  "agent": {"model": "swe-2-max"},
  "hooks": {"SessionStart": [{"hooks": [{"type": "command", "command": "stale"}]}]}
}
"#,
    )
    .unwrap();

    run_harness([
        "sync-devin-config",
        "--source",
        source_path.to_str().unwrap(),
        "--target",
        target_path.to_str().unwrap(),
    ]);

    let target = read_json(&target_path);
    assert_eq!(target["version"], 1);
    assert_eq!(target["devin"]["org_id"], "org-123");
    assert_eq!(target["agent"]["model"], "swe-2-max");
    assert_eq!(
        target["hooks"]["SessionStart"][0]["hooks"][0]["command"].as_str(),
        Some("generated"),
    );

    remove_dir(root);
}

#[test]
fn codex_config_sync_preserves_codex_owned_state() {
    let root = test_root("codex-sync");
    let source_path = root.join("source.toml");
    let target_path = root.join("target.toml");

    std::fs::write(&source_path, "model = \"source-model\"\n").unwrap();
    std::fs::write(
        &target_path,
        r#"
model = "target-model"
sandbox_mode = "read-only"

[projects."/tmp/project"]
trust_level = "trusted"
"#,
    )
    .unwrap();

    run_harness([
        "sync-codex-config",
        "--source",
        source_path.to_str().unwrap(),
        "--target",
        target_path.to_str().unwrap(),
    ]);

    let target = read_toml(&target_path);
    assert_eq!(target["model"].as_str(), Some("source-model"));
    assert!(target.get("sandbox_mode").is_none());
    assert_eq!(
        target["projects"]["/tmp/project"]["trust_level"].as_str(),
        Some("trusted"),
    );

    remove_dir(root);
}

#[test]
fn codex_config_sync_manages_only_the_agent_gh_config_dir_in_the_shell_environment() {
    let root = test_root("codex-sync-shell-environment");
    let source_path = root.join("source.toml");
    let target_path = root.join("target.toml");

    std::fs::write(
        &source_path,
        "[shell_environment_policy.set]\nGH_CONFIG_DIR = \"/agent/gh\"\n",
    )
    .unwrap();
    std::fs::write(
        &target_path,
        "[shell_environment_policy]\ninherit = \"core\"\n\n[shell_environment_policy.set]\nCODEX_OWNED = \"1\"\n",
    )
    .unwrap();
    let sync = || {
        run_harness([
            "sync-codex-config",
            "--source",
            source_path.to_str().unwrap(),
            "--target",
            target_path.to_str().unwrap(),
        ]);
    };

    sync();
    let target = read_toml(&target_path);
    let policy = &target["shell_environment_policy"];
    assert_eq!(policy["set"]["GH_CONFIG_DIR"].as_str(), Some("/agent/gh"));
    assert_eq!(policy["set"]["CODEX_OWNED"].as_str(), Some("1"));
    assert_eq!(policy["inherit"].as_str(), Some("core"));

    std::fs::write(&source_path, "").unwrap();
    sync();
    let target = read_toml(&target_path);
    let set = &target["shell_environment_policy"]["set"];
    assert!(set.get("GH_CONFIG_DIR").is_none());
    assert_eq!(set["CODEX_OWNED"].as_str(), Some("1"));

    remove_dir(root);
}
