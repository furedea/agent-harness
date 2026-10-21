use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

#[test]
fn write_claude_hooks_reads_shared_hook_config() -> Result<()> {
    let root = test_root("write_claude_hooks_reads_shared_hook_config")?;
    write_hook_config(&root)?;
    let output = root.join("claude-hooks.json");

    write_claude_hooks(&root, &output, &[])?;

    let hooks: Value = serde_json::from_str(&std::fs::read_to_string(&output)?)?;
    assert_eq!(
        hooks["PreToolUse"][0]["hooks"][0]["command"].as_str(),
        Some("$HOME/.claude/hooks/guard_forbidden_commands.sh"),
    );

    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn write_codex_hooks_reads_shared_hook_config() -> Result<()> {
    let root = test_root("write_codex_hooks_reads_shared_hook_config")?;
    write_hook_config(&root)?;
    let output = root.join("codex-hooks.json");

    write_codex_hooks(&root, &output, &[])?;

    let hooks: Value = serde_json::from_str(&std::fs::read_to_string(&output)?)?;
    assert_eq!(
        hooks["hooks"]["PreToolUse"][0]["matcher"].as_str(),
        Some("^(Bash|exec_command|functions\\.exec_command)$"),
    );

    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn write_devin_hooks_renders_the_devin_section_as_a_config_fragment() -> Result<()> {
    let root = test_root("write_devin_hooks_renders_the_devin_section_as_a_config_fragment")?;
    write_hook_config(&root)?;
    write_command_policy(&root)?;
    let output = root.join("devin-hooks.json");

    write_devin_hooks(&root, &output, &[])?;

    let hooks: Value = serde_json::from_str(&std::fs::read_to_string(&output)?)?;
    assert_eq!(
        hooks["hooks"]["PreToolUse"][0]["matcher"].as_str(),
        Some("exec"),
    );
    assert_eq!(
        hooks["hooks"]["PreToolUse"][0]["hooks"][0]["command"].as_str(),
        Some("$HOME/.devin/hooks/hook_adapter.py shell forbidden"),
    );

    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn write_devin_hooks_includes_command_permissions() -> Result<()> {
    let root = test_root("write_devin_hooks_includes_command_permissions")?;
    write_hook_config(&root)?;
    write_command_policy(&root)?;
    let output = root.join("devin-hooks.json");

    write_devin_hooks(&root, &output, &[])?;

    let config: Value = serde_json::from_str(&std::fs::read_to_string(&output)?)?;
    assert_eq!(
        config["permissions"],
        serde_json::json!({
            "allow": ["Exec(cargo)"],
            "ask": ["Exec(git push)"],
            "deny": ["Exec(curl)"]
        })
    );

    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn write_devin_hooks_emits_empty_hooks_when_the_section_is_missing() -> Result<()> {
    let root = test_root("write_devin_hooks_emits_empty_hooks_when_the_section_is_missing")?;
    write_file(
        &root.join("hooks.json"),
        r#"{"version":1,"claude":{},"codex":{"hooks":{}}}"#,
    )?;
    write_command_policy(&root)?;
    let output = root.join("devin-hooks.json");

    write_devin_hooks(&root, &output, &[])?;

    let hooks: Value = serde_json::from_str(&std::fs::read_to_string(&output)?)?;
    assert_eq!(hooks["hooks"], serde_json::json!({}));

    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn read_hooks_rejects_unsupported_version() -> Result<()> {
    let root = test_root("read_hooks_rejects_unsupported_version")?;
    write_file(
        &root.join("hooks.json"),
        r#"{"version":2,"claude":{},"codex":{}}"#,
    )?;

    let error = read_hooks(&root).unwrap_err().to_string();

    assert!(error.contains("unsupported hook config version"));

    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn claude_hooks_for_runtime_relocates_nested_commands_under_a_directory_root() -> Result<()> {
    let root = test_root("claude_hooks_for_runtime_relocates_nested_commands")?;
    write_hook_config(&root)?;
    let runtime_root = RuntimeRoot::directory(PathBuf::from("/opt/agents"))?;
    let context = GenerationContext {
        source: &root,
        external_hooks: &[],
        runtime_root: &runtime_root,
    };

    let hooks = claude_hooks_for_runtime(context)?;

    assert_eq!(
        hooks["PreToolUse"][0]["hooks"][0]["command"].as_str(),
        Some(
            "export AGENT_HARNESS_ROOT='/opt/agents'; '/opt/agents'/.claude/hooks/guard_forbidden_commands.sh"
        ),
    );

    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn built_in_hook_metadata_orders_providers_and_events() -> Result<()> {
    let root = test_root("built_in_hook_metadata_orders_providers_and_events")?;
    write_file(
        &root.join("hooks.json"),
        r#"{
  "version": 1,
  "claude": {
    "Stop": [{"hooks": [{"command": "stop.sh"}]}],
    "SessionStart": [{"matcher": "startup", "hooks": [{"command": "start.sh", "if": "Bash"}]}]
  },
  "codex": {"hooks": {}},
  "hermes": {"hooks": {"on_session_start": [{"hooks": [{"command": "hermes.sh"}]}]}},
  "pi": {"hooks": {"session_start": [{"hooks": [{"command": "pi.sh"}]}]}}
}"#,
    )?;

    let metadata = built_in_hook_metadata(&root)?;

    let summary = metadata
        .iter()
        .map(|entry| {
            format!(
                "{}:{}:{}:{}:{}",
                entry.provider.display_name(),
                entry.event,
                entry.matcher,
                entry.condition.as_deref().unwrap_or("-"),
                entry.command
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        summary,
        [
            "Claude:SessionStart:startup:Bash:start.sh",
            "Claude:Stop:*:-:stop.sh",
            "Hermes:on_session_start:*:-:hermes.sh",
            "Pi:session_start:*:-:pi.sh",
        ]
    );

    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn read_hooks_rejects_a_non_object_optional_section() -> Result<()> {
    let root = test_root("read_hooks_rejects_a_non_object_optional_section")?;
    write_file(
        &root.join("hooks.json"),
        r#"{"version":1,"claude":{},"codex":{},"pi":[]}"#,
    )?;

    let error = read_hooks(&root).unwrap_err().to_string();

    assert_eq!(error, "hook config pi section must be a JSON object");

    std::fs::remove_dir_all(root)?;
    Ok(())
}

fn test_root(name: &str) -> Result<PathBuf> {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let root = std::env::temp_dir().join(format!("agent-harness-{name}-{nanos}"));
    std::fs::create_dir_all(&root)?;
    Ok(root)
}

fn write_command_policy(root: &Path) -> Result<()> {
    write_file(
        &root.join("command_permissions.json"),
        r#"{
  "version": 1,
  "rules": [
    {
      "decision": "allow",
      "prefix": ["cargo"],
      "justification": "Allowed by the shared agent command permissions."
    },
    {
      "decision": "ask",
      "prefix": ["git", "push"],
      "justification": "Publishing changes requires confirmation."
    },
    {
      "decision": "deny",
      "prefix": ["curl"],
      "justification": "Do not fetch remote scripts or content."
    }
  ]
}
"#,
    )
}

fn write_hook_config(root: &Path) -> Result<()> {
    write_file(
        &root.join("hooks.json"),
        r#"{
  "version": 1,
  "claude": {
    "PreToolUse": [
      {
        "matcher": "Bash",
        "hooks": [
          {
            "command": "$HOME/.claude/hooks/guard_forbidden_commands.sh",
            "type": "command"
          }
        ]
      }
    ]
  },
  "codex": {
    "hooks": {
      "PreToolUse": [
        {
          "matcher": "^(Bash|exec_command|functions\\.exec_command)$",
          "hooks": [
            {
              "command": "$HOME/.codex/hooks/adapt_shell_command.sh",
              "statusMessage": "Checking command permissions",
              "timeout": 30,
              "type": "command"
            }
          ]
        }
      ]
    }
  },
  "devin": {
    "hooks": {
      "PreToolUse": [
        {
          "matcher": "exec",
          "hooks": [
            {
              "command": "$HOME/.devin/hooks/hook_adapter.py shell forbidden",
              "type": "command"
            }
          ]
        }
      ]
    }
  }
}
"#,
    )
}

fn write_file(path: &Path, content: &str) -> Result<()> {
    let parent = path.parent().context("test file path must have a parent")?;
    std::fs::create_dir_all(parent)?;
    std::fs::write(path, content)?;
    Ok(())
}
