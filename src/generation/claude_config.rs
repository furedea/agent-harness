use std::path::Path;

use anyhow::{Result, bail};
use serde_json::{Map, Value};

use crate::generation::{
    GenerationContext, claude_permissions::PermissionRules, command_permissions, hooks,
    secret_path_policy,
};
use crate::{file_op::FileOp, runtime_root::RuntimeRoot, source_model::Source};

pub(crate) fn settings_op(source: &Source, out: &Path) -> Result<FileOp> {
    settings_op_for_runtime(GenerationContext::home(source), out)
}

pub(crate) fn settings_op_for_runtime(
    context: GenerationContext<'_>,
    out: &Path,
) -> Result<FileOp> {
    FileOp::write_json(out, &settings_for_runtime(context)?)
}

/// The Claude settings: the source's base settings with the generated parts merged in.
pub(crate) fn settings_for_runtime(context: GenerationContext<'_>) -> Result<Value> {
    build_settings(context, context.source.claude_settings.clone())
}

fn build_settings(context: GenerationContext<'_>, mut settings: Value) -> Result<Value> {
    let root = object_mut(&mut settings, "Claude settings root")?;

    relocate_status_line(root, context.runtime_root)?;

    root.insert(
        "hooks".to_string(),
        hooks::claude_hooks_for_runtime(context)?,
    );
    merge_permissions(root, context.source)?;

    Ok(settings)
}

fn merge_permissions(root: &mut Map<String, Value>, source: &Source) -> Result<()> {
    let permissions = object_entry(root, "permissions")?;
    let mut rules = PermissionRules::parse(permissions)?;

    rules.extend_allow(command_permissions::claude_allow_permissions(source));
    rules.extend_ask(command_permissions::claude_ask_permissions(source));
    rules.extend_deny(secret_path_policy::claude_deny_permissions(source));
    rules.extend_deny(command_permissions::claude_deny_permissions(source));

    rules.write_into(permissions);
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use anyhow::Context;
    use serde_json::json;

    use super::*;
    use crate::shell::fixtures::load_source;

    #[test]
    fn build_settings_merges_hooks_command_permissions_and_secret_paths() -> Result<()> {
        let root = test_root("build_settings_merges_hooks_command_permissions_and_secret_paths")?;
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

        let loaded = load_source(&root)?;

        let settings = build_settings(GenerationContext::home(&loaded), base)?;

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
            array_strings(&settings["permissions"]["allow"]).contains(&"Bash(old:*)".to_string())
        );
        assert!(
            array_strings(&settings["permissions"]["ask"]).contains(&"Read(docs/**)".to_string())
        );
        assert!(
            array_strings(&settings["permissions"]["ask"]).contains(&"Bash(old-ask:*)".to_string())
        );
        assert!(
            array_strings(&settings["permissions"]["deny"]).contains(&"Bash(curl:*)".to_string())
        );
        assert!(
            array_strings(&settings["permissions"]["deny"])
                .contains(&"Bash(old-deny:*)".to_string())
        );
        assert_eq!(settings["permissions"]["defaultMode"], "auto");
        assert!(
            array_strings(&settings["permissions"]["deny"]).contains(&"Read(.env*)".to_string())
        );
        assert!(
            !array_strings(&settings["permissions"]["deny"])
                .iter()
                .any(|rule| rule.contains("/.claude/")),
            "installed harness files are not denied by the harness"
        );
        assert_eq!(
            settings["sandbox"],
            json!({"filesystem": {"allowWrite": ["$HOME/.cache/nix"]}})
        );

        crate::shell::fixtures::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn build_settings_never_widens_a_rule_declared_by_both_source_and_generation() -> Result<()> {
        let root =
            test_root("build_settings_never_widens_a_rule_declared_by_both_source_and_generation")?;
        write_minimal_source(&root)?;
        let base = json!({
            "permissions": {
                "allow": ["Bash(curl:*)"],
                "ask": ["Bash(cargo:*)"]
            }
        });

        let loaded = load_source(&root)?;

        let settings = build_settings(GenerationContext::home(&loaded), base)?;

        let allow = array_strings(&settings["permissions"]["allow"]);
        let ask = array_strings(&settings["permissions"]["ask"]);
        let deny = array_strings(&settings["permissions"]["deny"]);
        assert!(!allow.contains(&"Bash(curl:*)".to_string()));
        assert!(deny.contains(&"Bash(curl:*)".to_string()));
        assert!(!allow.contains(&"Bash(cargo:*)".to_string()));
        assert!(ask.contains(&"Bash(cargo:*)".to_string()));

        crate::shell::fixtures::remove_dir_all(root)?;
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
        crate::shell::fixtures::create_dir_all(&root)?;
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
        let parent = path.parent().context("test file path must have a parent")?;
        crate::shell::fixtures::create_dir_all(parent)?;
        crate::shell::fixtures::write(path, content)?;
        Ok(())
    }
}
