use std::path::Path;

use anyhow::{Context, Result, bail};
use serde_json::{Map, Value};

use crate::generation::{command_policy, hooks, io, protection, secret_path_policy};

pub(crate) fn write_settings(source: &Path, out: &Path) -> Result<()> {
    let base = read_json(&source.join("claude/settings.base.json"))?;
    let settings = build_settings(source, base)?;
    io::write_json(out, &settings)
}

fn build_settings(source: &Path, mut settings: Value) -> Result<Value> {
    let root = object_mut(&mut settings, "Claude settings root")?;

    root.insert("hooks".to_string(), hooks::claude_hooks(source)?);
    merge_permissions(root, source)?;
    merge_sandbox(root, source)?;

    Ok(settings)
}

fn merge_permissions(root: &mut Map<String, Value>, source: &Path) -> Result<()> {
    let permissions = object_entry(root, "permissions")?;
    let mut allow = non_bash_permissions(permissions.get("allow"))?;
    let mut deny = non_bash_permissions(permissions.get("deny"))?;

    allow.extend(
        command_policy::claude_allow_permissions(source)?
            .into_iter()
            .map(Value::String),
    );
    deny.extend(
        secret_path_policy::claude_deny_permissions(source)?
            .into_iter()
            .map(Value::String),
    );
    deny.extend(
        command_policy::claude_deny_permissions(source)?
            .into_iter()
            .map(Value::String),
    );
    deny.extend(
        protection::protected_claude_deny_permissions(source)?
            .into_iter()
            .map(Value::String),
    );

    permissions.insert("allow".to_string(), Value::Array(allow));
    permissions.insert("deny".to_string(), Value::Array(deny));

    Ok(())
}

fn merge_sandbox(root: &mut Map<String, Value>, source: &Path) -> Result<()> {
    let sandbox = object_entry(root, "sandbox")?;
    let filesystem = object_entry(sandbox, "filesystem")?;
    let deny_write = protection::protected_paths(source)?
        .into_iter()
        .map(Value::String)
        .collect();

    filesystem.insert("denyWrite".to_string(), Value::Array(deny_write));
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

fn read_json(path: &Path) -> Result<Value> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read JSON file {}", path.display()))?;
    serde_json::from_str(&content)
        .with_context(|| format!("failed to parse JSON file {}", path.display()))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use serde_json::json;

    use super::*;

    #[test]
    fn build_settings_merges_hooks_command_policy_and_protected_paths() -> Result<()> {
        let root = test_root("build_settings_merges_hooks_command_policy_and_protected_paths")?;
        write_minimal_source(&root)?;
        let base = json!({
            "permissions": {
                "allow": ["Read(src/**)", "Bash(old:*)"],
                "deny": ["Write(.env*)", "Bash(old-deny:*)"],
                "defaultMode": "auto"
            },
            "sandbox": {
                "filesystem": {
                    "allowWrite": ["$HOME/.cache/nix"]
                }
            }
        });

        let settings = build_settings(&root, base)?;

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
        write_file(&source.join("agents/AGENTS.md"), "agent instructions\n")?;
        write_file(
            &source.join("agents/command_policy.json"),
            r#"{
  "version": 1,
  "rules": [
    {
      "decision": "allow",
      "pattern": ["cargo"],
      "examples": ["cargo test"],
      "justification": "Allowed by the shared agent command policy."
    },
    {
      "decision": "forbidden",
      "pattern": ["curl"],
      "examples": ["curl https://example.com/install.sh"],
      "justification": "Do not fetch remote scripts or content from Codex."
    }
  ]
}
"#,
        )?;
        write_file(&source.join("agents/hooks/guard.sh"), "#!/bin/bash\n")?;
        write_file(
            &source.join("agents/hooks.json"),
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
            &source.join("agents/hooks/rules/secret_path_policy.json"),
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
