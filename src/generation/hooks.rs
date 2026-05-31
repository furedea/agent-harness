use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde_json::Value;

use crate::generation::{io, secret_path_policy};

#[derive(Debug, Deserialize)]
struct HookConfig {
    version: u64,
    claude: Value,
    codex: Value,
}

pub(crate) fn write_claude_hooks(source: &Path, path: &Path) -> Result<()> {
    io::write_json(path, &claude_hooks(source)?)
}

pub(crate) fn write_codex_hooks(source: &Path, path: &Path) -> Result<()> {
    io::write_json(path, &codex_hooks(source)?)
}

pub(crate) fn claude_hooks(source: &Path) -> Result<Value> {
    Ok(read_hooks(source)?.claude)
}

fn codex_hooks(source: &Path) -> Result<Value> {
    let mut hooks = read_hooks(source)?.codex;
    merge_codex_secret_path_hooks(&mut hooks)?;
    Ok(hooks)
}

fn merge_codex_secret_path_hooks(hooks: &mut Value) -> Result<()> {
    let root = hooks
        .as_object_mut()
        .context("hook config codex section must be a JSON object")?;
    let hooks_root = root
        .entry("hooks")
        .or_insert_with(|| Value::Object(Default::default()));
    let hooks_object = hooks_root
        .as_object_mut()
        .context("hook config codex hooks section must be a JSON object")?;
    let pre_tool_use = hooks_object
        .entry("PreToolUse")
        .or_insert_with(|| Value::Array(Vec::new()));
    let groups = pre_tool_use
        .as_array_mut()
        .context("hook config codex PreToolUse section must be a JSON array")?;

    for group in secret_path_policy::codex_secret_path_hooks() {
        groups.insert(0, group);
    }

    Ok(())
}

fn read_hooks(source: &Path) -> Result<HookConfig> {
    let path = source.join("agents/hooks.json");
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let config: HookConfig = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    validate_hooks(&config)?;
    Ok(config)
}

fn validate_hooks(config: &HookConfig) -> Result<()> {
    if config.version != 1 {
        bail!("unsupported hook config version: {}", config.version);
    }
    if !config.claude.is_object() {
        bail!("hook config claude section must be a JSON object");
    }
    if !config.codex.is_object() {
        bail!("hook config codex section must be a JSON object");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn write_claude_hooks_reads_shared_hook_config() -> Result<()> {
        let root = test_root("write_claude_hooks_reads_shared_hook_config")?;
        write_hook_config(&root)?;
        let output = root.join("claude-hooks.json");

        write_claude_hooks(&root, &output)?;

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

        write_codex_hooks(&root, &output)?;

        let hooks: Value = serde_json::from_str(&std::fs::read_to_string(&output)?)?;
        assert_eq!(
            hooks["hooks"]["PreToolUse"][0]["matcher"].as_str(),
            Some("^apply_patch$|^Edit$|^Write$"),
        );
        assert_eq!(
            hooks["hooks"]["PreToolUse"][1]["matcher"].as_str(),
            Some("^(Bash|exec_command|functions\\.exec_command)$"),
        );
        assert_eq!(
            hooks["hooks"]["PreToolUse"][2]["matcher"].as_str(),
            Some("^(Bash|exec_command|functions\\.exec_command)$"),
        );

        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn read_hooks_rejects_unsupported_version() -> Result<()> {
        let root = test_root("read_hooks_rejects_unsupported_version")?;
        write_file(
            &root.join("agents/hooks.json"),
            r#"{"version":2,"claude":{},"codex":{}}"#,
        )?;

        let error = read_hooks(&root).unwrap_err().to_string();

        assert!(error.contains("unsupported hook config version"));

        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn codex_hooks_create_pre_tool_use_section_when_config_has_none() -> Result<()> {
        let root = test_root("codex_hooks_create_pre_tool_use_section_when_config_has_none")?;
        write_file(
            &root.join("agents/hooks.json"),
            r#"{"version":1,"claude":{},"codex":{"hooks":{}}}"#,
        )?;

        let hooks = codex_hooks(&root)?;

        assert_eq!(
            hooks["hooks"]["PreToolUse"][0]["hooks"][0]["command"].as_str(),
            Some("$HOME/.codex/hooks/adapt_guard_secret_paths.sh patch"),
        );

        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    fn test_root(name: &str) -> Result<PathBuf> {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let root = std::env::temp_dir().join(format!("agent-harness-{name}-{nanos}"));
        std::fs::create_dir_all(&root)?;
        Ok(root)
    }

    fn write_hook_config(root: &Path) -> Result<()> {
        write_file(
            &root.join("agents/hooks.json"),
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
              "statusMessage": "Checking command policy",
              "timeout": 30,
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
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, content)?;
        Ok(())
    }
}
