use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

#[test]
fn claude_settings_sync_expands_excluded_command_home_paths() {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "agent-harness-claude-sync-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let home = root.join("custom home");
    let source_path = root.join("source.json");
    let target_path = root.join("target.json");
    let cases = [
        (
            "~/.local/bin/tool *",
            format!("{}/.local/bin/tool *", home.display()),
        ),
        (
            r#"node "~/.claude/plugins/codex-companion.mjs" *"#,
            format!(
                r#"node "{}/.claude/plugins/codex-companion.mjs" *"#,
                home.display()
            ),
        ),
        (
            r#"node "~/first" "~/second" *"#,
            format!(r#"node "{0}/first" "{0}/second" *"#, home.display()),
        ),
        ("git *", "git *".to_string()),
        ("/usr/bin/tool *", "/usr/bin/tool *".to_string()),
        ("node ~/script *", "node ~/script *".to_string()),
        ("path/~/tool *", "path/~/tool *".to_string()),
        ("~other/tool *", "~other/tool *".to_string()),
        ("~", "~".to_string()),
        ("", String::new()),
    ];
    let source = serde_json::json!({
        "sandbox": {
            "excludedCommands": cases.iter().map(|(input, _)| *input).collect::<Vec<_>>(),
            "filesystem": {"denyWrite": ["~/protected"]}
        },
        "statusLine": {"command": "~/status.sh"}
    });
    std::fs::write(&source_path, serde_json::to_vec(&source).unwrap()).unwrap();
    std::fs::write(&target_path, r#"{"model":"user-model"}"#).unwrap();

    let sync = || {
        let output = Command::new(env!("CARGO_BIN_EXE_agent-harness"))
            .env("HOME", &home)
            .args([
                "sync-claude-settings",
                "--source",
                source_path.to_str().unwrap(),
                "--target",
                target_path.to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    sync();

    let mut expected = source.clone();
    expected["sandbox"]["excludedCommands"] =
        serde_json::json!(cases.iter().map(|(_, output)| output).collect::<Vec<_>>());
    expected["model"] = serde_json::json!("user-model");
    assert_eq!(read_json(&target_path), expected);
    assert_eq!(read_json(&source_path), source);
    sync();
    assert_eq!(read_json(&target_path), expected);

    std::fs::remove_dir_all(root).unwrap();
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}
