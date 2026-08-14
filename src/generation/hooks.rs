use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde_json::Value;

use crate::generation::{herdr, io};

#[derive(Debug, Deserialize)]
struct HookConfig {
    version: u64,
    claude: Value,
    codex: Value,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum HookProvider {
    Claude,
    Codex,
}

impl HookProvider {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct HookMetadata {
    pub(crate) provider: HookProvider,
    pub(crate) event: String,
    pub(crate) matcher: String,
    pub(crate) condition: Option<String>,
    pub(crate) command: String,
}

pub(crate) fn write_claude_hooks(source: &Path, path: &Path) -> Result<()> {
    write_claude_hooks_with_herdr(source, path, None)
}

#[cfg(test)]
pub(crate) fn write_codex_hooks(source: &Path, path: &Path) -> Result<()> {
    write_codex_hooks_with_herdr(source, path, None)
}

pub(crate) fn write_claude_hooks_with_herdr(
    source: &Path,
    path: &Path,
    integration: Option<&Path>,
) -> Result<()> {
    io::write_json(path, &claude_hooks_with_herdr(source, integration)?)
}

pub(crate) fn write_codex_hooks_with_herdr(
    source: &Path,
    path: &Path,
    integration: Option<&Path>,
) -> Result<()> {
    io::write_json(path, &codex_hooks_with_herdr(source, integration)?)
}

pub(crate) fn claude_hooks_with_herdr(source: &Path, integration: Option<&Path>) -> Result<Value> {
    herdr::merge_claude_hooks(read_hooks(source)?.claude, integration)
}

pub(crate) fn built_in_hook_metadata(source: &Path) -> Result<Vec<HookMetadata>> {
    let config = read_hooks(source)?;
    let codex = config
        .codex
        .get("hooks")
        .context("hook config codex section must contain hooks")?;
    let mut metadata = hook_metadata(HookProvider::Claude, &config.claude)?;
    metadata.extend(hook_metadata(HookProvider::Codex, codex)?);
    metadata.sort();
    Ok(metadata)
}

fn codex_hooks_with_herdr(source: &Path, integration: Option<&Path>) -> Result<Value> {
    herdr::merge_codex_hooks(read_hooks(source)?.codex, integration)
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

fn hook_metadata(provider: HookProvider, value: &Value) -> Result<Vec<HookMetadata>> {
    let events = value
        .as_object()
        .context("hook provider section must be a JSON object")?;
    let mut metadata = Vec::new();

    for (event, groups) in events {
        let groups = groups
            .as_array()
            .with_context(|| format!("hook event {event} must be a JSON array"))?;
        for group in groups {
            metadata.extend(group_hook_metadata(provider, event, group)?);
        }
    }
    Ok(metadata)
}

fn group_hook_metadata(
    provider: HookProvider,
    event: &str,
    group: &Value,
) -> Result<Vec<HookMetadata>> {
    let group = group
        .as_object()
        .with_context(|| format!("hook group for {event} must be a JSON object"))?;
    let matcher = group
        .get("matcher")
        .and_then(Value::as_str)
        .unwrap_or("*")
        .to_owned();
    let hooks = group
        .get("hooks")
        .and_then(Value::as_array)
        .with_context(|| format!("hook group for {event} must contain a hooks array"))?;

    hooks
        .iter()
        .map(|hook| hook_metadata_entry(provider, event, &matcher, hook))
        .collect()
}

fn hook_metadata_entry(
    provider: HookProvider,
    event: &str,
    matcher: &str,
    hook: &Value,
) -> Result<HookMetadata> {
    let hook = hook
        .as_object()
        .with_context(|| format!("hook entry for {event} must be a JSON object"))?;
    let command = hook
        .get("command")
        .and_then(Value::as_str)
        .with_context(|| format!("hook entry for {event} must contain a command"))?;
    let condition = hook.get("if").and_then(Value::as_str).map(str::to_owned);
    Ok(HookMetadata {
        provider,
        event: event.to_owned(),
        matcher: matcher.to_owned(),
        condition,
        command: command.to_owned(),
    })
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
