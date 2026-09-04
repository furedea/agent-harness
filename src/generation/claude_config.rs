use std::path::Path;

use anyhow::{Context, Result, bail};
use serde_json::{Map, Value};

use crate::generation::{
    command_permissions, external_hooks::ExternalHookBundle, hooks, io, protection,
    secret_path_policy,
};
use crate::{fs_ops, layout::SourceLayout, runtime_root::RuntimeRoot};

pub(crate) fn write_settings(
    source: &Path,
    out: &Path,
    external_hooks: &[ExternalHookBundle],
) -> Result<()> {
    write_settings_for_runtime(source, out, external_hooks, &RuntimeRoot::home())
}

pub(crate) fn write_settings_for_runtime(
    source: &Path,
    out: &Path,
    external_hooks: &[ExternalHookBundle],
    runtime_root: &RuntimeRoot,
) -> Result<()> {
    let base = read_json(&SourceLayout::new(source).claude_settings())?;
    let settings = build_settings(source, base, external_hooks, runtime_root)?;
    io::write_json(out, &settings)
}

pub(crate) fn sync_settings(source_path: &Path, target_path: &Path) -> Result<()> {
    let generated = read_json(source_path)?;
    let mut existing = read_json_or_empty(target_path)?;
    merge_managed_settings(&mut existing, generated)?;
    let content = serde_json::to_string_pretty(&existing)? + "\n";
    fs_ops::write_file_atomically(target_path, content.as_bytes())
}

fn build_settings(
    source: &Path,
    mut settings: Value,
    external_hooks: &[ExternalHookBundle],
    runtime_root: &RuntimeRoot,
) -> Result<Value> {
    let root = object_mut(&mut settings, "Claude settings root")?;

    relocate_status_line(root, runtime_root)?;

    root.insert(
        "hooks".to_string(),
        hooks::claude_hooks_for_runtime(source, external_hooks, runtime_root)?,
    );
    merge_permissions(root, source, external_hooks, runtime_root)?;
    merge_sandbox(root, source, external_hooks, runtime_root)?;

    Ok(settings)
}

fn merge_permissions(
    root: &mut Map<String, Value>,
    source: &Path,
    external_hooks: &[ExternalHookBundle],
    runtime_root: &RuntimeRoot,
) -> Result<()> {
    let permissions = object_entry(root, "permissions")?;
    let mut allow = non_bash_permissions(permissions.get("allow"))?;
    let mut ask = non_bash_permissions(permissions.get("ask"))?;
    let mut deny = non_bash_permissions(permissions.get("deny"))?;

    allow.extend(
        command_permissions::claude_allow_permissions(source)?
            .into_iter()
            .map(Value::String),
    );
    ask.extend(
        command_permissions::claude_ask_permissions(source)?
            .into_iter()
            .map(Value::String),
    );
    deny.extend(
        secret_path_policy::claude_deny_permissions(source)?
            .into_iter()
            .map(Value::String),
    );
    deny.extend(
        command_permissions::claude_deny_permissions(source)?
            .into_iter()
            .map(Value::String),
    );
    deny.extend(
        protection::protected_paths_for_runtime(source, external_hooks, runtime_root)?
            .into_iter()
            .map(|path| format!("Edit({path})"))
            .map(Value::String),
    );

    permissions.insert("allow".to_string(), Value::Array(allow));
    permissions.insert("ask".to_string(), Value::Array(ask));
    permissions.insert("deny".to_string(), Value::Array(deny));

    Ok(())
}

fn merge_sandbox(
    root: &mut Map<String, Value>,
    source: &Path,
    external_hooks: &[ExternalHookBundle],
    runtime_root: &RuntimeRoot,
) -> Result<()> {
    let sandbox = object_entry(root, "sandbox")?;
    let filesystem = object_entry(sandbox, "filesystem")?;
    let deny_write = protection::protected_paths_for_runtime(source, external_hooks, runtime_root)?
        .into_iter()
        .map(Value::String)
        .collect();

    filesystem.insert("denyWrite".to_string(), Value::Array(deny_write));
    Ok(())
}

fn relocate_status_line(root: &mut Map<String, Value>, runtime_root: &RuntimeRoot) -> Result<()> {
    let Some(status_line) = root.get_mut("statusLine") else {
        return Ok(());
    };
    let status_line = object_mut(status_line, "statusLine")?;
    if let Some(Value::String(command)) = status_line.get_mut("command") {
        *command = runtime_root.relocate_command(command);
    }
    Ok(())
}

fn object_entry<'a>(
    root: &'a mut Map<String, Value>,
    key: &str,
) -> Result<&'a mut Map<String, Value>> {
    let entry = root
        .entry(key.to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    object_mut(entry, key)
}

fn object_mut<'a>(value: &'a mut Value, name: &str) -> Result<&'a mut Map<String, Value>> {
    match value {
        Value::Object(object) => Ok(object),
        _ => bail!("{name} must be a JSON object"),
    }
}

fn non_bash_permissions(value: Option<&Value>) -> Result<Vec<Value>> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let Some(values) = value.as_array() else {
        bail!("permissions entries must be JSON arrays");
    };

    Ok(values
        .iter()
        .filter(|entry| {
            !entry
                .as_str()
                .is_some_and(|permission| permission.starts_with("Bash("))
        })
        .cloned()
        .collect())
}

fn merge_managed_settings(existing: &mut Value, generated: Value) -> Result<()> {
    let existing = object_mut(existing, "existing Claude settings root")?;
    let Value::Object(generated) = generated else {
        bail!("generated Claude settings root must be a JSON object");
    };
    existing.extend(generated);
    Ok(())
}

fn read_json(path: &Path) -> Result<Value> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read JSON file {}", path.display()))?;
    serde_json::from_str(&content)
        .with_context(|| format!("failed to parse JSON file {}", path.display()))
}

fn read_json_or_empty(path: &Path) -> Result<Value> {
    match std::fs::read_to_string(path) {
        Ok(content) => serde_json::from_str(&content)
            .with_context(|| format!("failed to parse JSON file {}", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Value::Object(Map::new())),
        Err(error) => {
            Err(error).with_context(|| format!("failed to read JSON file {}", path.display()))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use serde_json::json;

    use super::*;

    #[test]
    fn build_settings_merges_hooks_command_permissions_and_protected_paths() -> Result<()> {
        let root =
            test_root("build_settings_merges_hooks_command_permissions_and_protected_paths")?;
        write_minimal_source(&root)?;
        let base = json!({
            "permissions": {
                "allow": ["Read(src/**)", "Bash(old:*)"],
                "ask": ["Read(docs/**)", "Bash(old-ask:*)"],
                "deny": ["Edit(.env*)", "Bash(old-deny:*)"],
                "defaultMode": "auto"
            },
            "sandbox": {
                "filesystem": {
                    "allowWrite": ["$HOME/.cache/nix"]
                }
            }
        });

        let settings = build_settings(&root, base, &[], &RuntimeRoot::home())?;

        assert_eq!(
            settings["hooks"]["PreToolUse"][0]["hooks"][1]["command"].as_str(),
            Some("$HOME/.claude/hooks/guard_forbidden_commands.sh"),
        );
        assert!(
            array_strings(&settings["permissions"]["allow"]).contains(&"Read(src/**)".to_string())
        );
        assert!(
            array_strings(&settings["permissions"]["allow"]).contains(&"Bash(cargo:*)".to_string())
        );
        assert!(
            !array_strings(&settings["permissions"]["allow"]).contains(&"Bash(old:*)".to_string())
        );
        assert!(
            array_strings(&settings["permissions"]["ask"]).contains(&"Read(docs/**)".to_string())
        );
        assert!(
            !array_strings(&settings["permissions"]["ask"])
                .contains(&"Bash(old-ask:*)".to_string())
        );
        assert!(
            array_strings(&settings["permissions"]["deny"]).contains(&"Bash(curl:*)".to_string())
        );
        assert!(
            array_strings(&settings["permissions"]["deny"]).contains(&"Read(.env*)".to_string())
        );
        assert!(
            array_strings(&settings["sandbox"]["filesystem"]["denyWrite"])
                .contains(&"~/.claude/hooks/guard.sh".to_string())
        );

        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn sync_settings_applies_generated_top_level_ownership() -> Result<()> {
        let root = test_root("sync-settings")?;
        let source = root.join("generated.json");
        let target = root.join("settings.json");
        let existing = json!({
            "model": "provider-model",
            "permissions": {
                "allow": ["provider-command"],
                "providerOnly": true
            }
        });
        let generated = json!({
            "hooks": {"PreToolUse": ["generated-hook"]},
            "permissions": {"allow": ["generated-command"]}
        });
        write_file(&source, &serde_json::to_string(&generated)?)?;
        write_file(&target, &serde_json::to_string(&existing)?)?;

        sync_settings(&source, &target)?;

        assert_eq!(
            read_json(&target)?,
            json!({
                "model": "provider-model",
                "hooks": {"PreToolUse": ["generated-hook"]},
                "permissions": {"allow": ["generated-command"]}
            }),
        );
        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn sync_settings_creates_a_missing_target() -> Result<()> {
        let root = test_root("sync-settings-missing-target")?;
        let source = root.join("generated.json");
        let target = root.join("home/settings.json");
        let generated = json!({"hooks": {}, "permissions": {"allow": []}});
        write_file(&source, &serde_json::to_string(&generated)?)?;

        sync_settings(&source, &target)?;

        assert_eq!(read_json(&target)?, generated);
        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    fn array_strings(value: &Value) -> Vec<String> {
        value
            .as_array()
            .unwrap_or(&Vec::new())
            .iter()
            .filter_map(|value| value.as_str().map(ToOwned::to_owned))
            .collect()
    }

    fn test_root(name: &str) -> Result<PathBuf> {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let root = std::env::temp_dir().join(format!("agent-harness-{name}-{nanos}"));
        std::fs::create_dir_all(&root)?;
        Ok(root)
    }

    fn write_minimal_source(source: &Path) -> Result<()> {
        write_file(
            &source.join("manifest.json"),
            r#"{"version":1,"runtime_commands":[]}"#,
        )?;
        write_file(&source.join("AGENTS.md"), "agent instructions\n")?;
        write_file(
            &source.join("command_permissions.json"),
            r#"{
  "version": 1,
  "rules": [
    {
      "decision": "allow",
      "prefix": ["cargo"],
      "justification": "Allowed by the shared agent command permissions."
    },
    {
      "decision": "deny",
      "prefix": ["curl"],
      "justification": "Do not fetch remote scripts or content from Codex."
    }
  ]
}
"#,
        )?;
        write_file(&source.join("hooks/guard.sh"), "#!/bin/bash\n")?;
        write_file(
            &source.join("hooks.json"),
            r#"{
  "version": 1,
  "claude": {
    "PreToolUse": [
      {
        "matcher": "Bash",
        "hooks": [
          {
            "command": "$HOME/.claude/hooks/audit_tool_call.sh",
            "type": "command"
          },
          {
            "command": "$HOME/.claude/hooks/guard_forbidden_commands.sh",
            "type": "command"
          }
        ]
      }
    ]
  },
  "codex": {
    "hooks": {}
  }
}
"#,
        )?;
        write_file(&source.join("codex/hooks/adapt.sh"), "#!/bin/bash\n")?;
        write_file(
            &source.join("hooks/rules/secret_path_policy.json"),
            r#"{
  "version": 1,
  "rules": [
    {
      "pattern": ".env*",
      "access": ["read"],
      "reason": "Environment files may contain credentials."
    }
  ]
}
"#,
        )?;
        Ok(())
    }

    fn write_file(path: &Path, content: &str) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, content)?;
        Ok(())
    }
}
