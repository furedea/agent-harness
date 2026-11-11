use anyhow::{Result, bail};
use serde_json::{Map, Value};

use crate::{
    file_op::FileOp,
    generation::json_documents::{read_json, read_json_or_empty},
    snapshot::ConfigFiles,
};

pub(crate) fn sync_config_op(config: ConfigFiles<'_>) -> Result<FileOp> {
    let generated = read_json(config.files, config.generated)?;
    let mut existing = read_json_or_empty(config.files, config.target)?;
    merge_managed_config(&mut existing, generated)?;
    FileOp::write_json(config.target, &existing)
}

fn merge_managed_config(existing: &mut Value, generated: Value) -> Result<()> {
    let existing = object_mut(existing, "existing Devin config root")?;
    let Value::Object(mut generated) = generated else {
        bail!("generated Devin config root must be a JSON object");
    };
    if let Some(permissions) = generated.remove("permissions") {
        merge_permissions(existing, permissions)?;
    }
    existing.extend(generated);
    Ok(())
}

// Exec(...) entries are managed by agent-harness, the same way Bash(...)
// entries are managed in Claude settings. Other entry kinds stay user-owned.
fn merge_permissions(existing: &mut Map<String, Value>, generated: Value) -> Result<()> {
    let Value::Object(generated) = generated else {
        bail!("generated Devin permissions must be a JSON object");
    };
    let permissions = existing
        .entry("permissions".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    let permissions = object_mut(permissions, "existing Devin permissions")?;

    for (key, value) in generated {
        merge_permission_entries(permissions, key, value)?;
    }
    Ok(())
}

fn merge_permission_entries(
    permissions: &mut Map<String, Value>,
    key: String,
    value: Value,
) -> Result<()> {
    let Value::Array(generated_entries) = value else {
        bail!("generated Devin permissions.{key} must be a JSON array");
    };
    let mut entries = user_owned_entries(permissions.get(&key))?;
    entries.extend(generated_entries);
    permissions.insert(key, Value::Array(entries));
    Ok(())
}

fn user_owned_entries(value: Option<&Value>) -> Result<Vec<Value>> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let Some(values) = value.as_array() else {
        bail!("existing Devin permissions entries must be JSON arrays");
    };

    Ok(values
        .iter()
        .filter(|entry| {
            !entry
                .as_str()
                .is_some_and(|permission| permission.starts_with("Exec("))
        })
        .cloned()
        .collect())
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

    use serde_json::json;

    use super::*;
    use crate::shell::fixtures::load_documents;
    use std::path::Path;

    fn sync_config(source: &Path, target: &Path) -> Result<()> {
        let files = load_documents(&[source, target])?;
        let config = ConfigFiles {
            generated: source,
            target,
            files: &files,
        };
        crate::shell::execute(&[sync_config_op(config)?])
    }

    fn read_json_file(path: &Path) -> Result<Value> {
        read_json(&load_documents(&[path])?, path)
    }

    #[test]
    fn sync_config_applies_generated_top_level_ownership() -> Result<()> {
        let root = test_root("sync-devin-config")?;
        let source = root.join("generated.json");
        let target = root.join("config.json");
        let existing = json!({
            "version": 1,
            "devin": {"org_id": "org-123"},
            "agent": {"model": "swe-2-max"},
            "hooks": {"SessionStart": ["user-hook"]}
        });
        let generated = json!({
            "hooks": {"SessionStart": ["generated-hook"]}
        });
        write_file(&source, &serde_json::to_string(&generated)?)?;
        write_file(&target, &serde_json::to_string(&existing)?)?;

        sync_config(&source, &target)?;

        assert_eq!(
            read_json_file(&target)?,
            json!({
                "version": 1,
                "devin": {"org_id": "org-123"},
                "agent": {"model": "swe-2-max"},
                "hooks": {"SessionStart": ["generated-hook"]}
            }),
        );
        crate::shell::fixtures::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn sync_config_merges_permissions_and_preserves_user_entries() -> Result<()> {
        let root = test_root("sync-devin-config-permissions")?;
        let source = root.join("generated.json");
        let target = root.join("config.json");
        let existing = json!({
            "version": 1,
            "agent": {"model": "swe-2-max"},
            "permissions": {
                "allow": ["Exec(ls)", "Read(**)", "mcp__github__*"],
                "deny": ["Exec(rm)", "Write(.env*)"]
            }
        });
        let generated = json!({
            "hooks": {},
            "permissions": {
                "allow": ["Exec(cargo)", "Exec(git status)"],
                "ask": ["Exec(git push)"],
                "deny": ["Exec(curl)"]
            }
        });
        write_file(&source, &serde_json::to_string(&generated)?)?;
        write_file(&target, &serde_json::to_string(&existing)?)?;

        sync_config(&source, &target)?;

        assert_eq!(
            read_json_file(&target)?,
            json!({
                "version": 1,
                "agent": {"model": "swe-2-max"},
                "hooks": {},
                "permissions": {
                    "allow": ["Read(**)", "mcp__github__*", "Exec(cargo)", "Exec(git status)"],
                    "ask": ["Exec(git push)"],
                    "deny": ["Write(.env*)", "Exec(curl)"]
                }
            }),
        );
        crate::shell::fixtures::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn sync_config_permissions_merge_is_idempotent() -> Result<()> {
        let root = test_root("sync-devin-config-permissions-idempotent")?;
        let source = root.join("generated.json");
        let target = root.join("config.json");
        let generated = json!({
            "permissions": {
                "allow": ["Exec(cargo)"],
                "ask": [],
                "deny": ["Exec(curl)"]
            }
        });
        write_file(&source, &serde_json::to_string(&generated)?)?;

        sync_config(&source, &target)?;
        let first = read_json_file(&target)?;
        sync_config(&source, &target)?;

        assert_eq!(read_json_file(&target)?, first);
        assert_eq!(first["permissions"]["allow"], json!(["Exec(cargo)"]),);
        crate::shell::fixtures::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn sync_config_without_generated_permissions_keeps_existing_permissions() -> Result<()> {
        let root = test_root("sync-devin-config-no-permissions")?;
        let source = root.join("generated.json");
        let target = root.join("config.json");
        let existing = json!({
            "permissions": {"allow": ["Exec(ls)", "Read(**)"]}
        });
        let generated = json!({"hooks": {}});
        write_file(&source, &serde_json::to_string(&generated)?)?;
        write_file(&target, &serde_json::to_string(&existing)?)?;

        sync_config(&source, &target)?;

        assert_eq!(
            read_json_file(&target)?["permissions"],
            json!({"allow": ["Exec(ls)", "Read(**)"]}),
        );
        crate::shell::fixtures::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn sync_config_creates_a_missing_target() -> Result<()> {
        let root = test_root("sync-devin-config-missing-target")?;
        let source = root.join("generated.json");
        let target = root.join("home/config.json");
        let generated = json!({"hooks": {}});
        write_file(&source, &serde_json::to_string(&generated)?)?;

        sync_config(&source, &target)?;

        assert_eq!(read_json_file(&target)?, generated);
        crate::shell::fixtures::remove_dir_all(root)?;
        Ok(())
    }

    fn test_root(name: &str) -> Result<PathBuf> {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let root = std::env::temp_dir().join(format!("agent-harness-{name}-{nanos}"));
        crate::shell::fixtures::create_dir_all(&root)?;
        Ok(root)
    }

    fn write_file(path: &Path, content: &str) -> Result<()> {
        path.parent()
            .map(crate::shell::fixtures::create_dir_all)
            .transpose()?;
        crate::shell::fixtures::write(path, content)?;
        Ok(())
    }
}
