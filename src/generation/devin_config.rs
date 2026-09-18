use std::path::Path;

use anyhow::{Context, Result, bail};
use serde_json::{Map, Value};

use crate::fs_ops;

pub(crate) fn sync_config(source_path: &Path, target_path: &Path) -> Result<()> {
    let generated = read_json(source_path)?;
    let mut existing = read_json_or_empty(target_path)?;
    merge_managed_config(&mut existing, generated)?;
    let content = serde_json::to_string_pretty(&existing)? + "\n";
    fs_ops::write_file_atomically(target_path, content.as_bytes())
}

fn merge_managed_config(existing: &mut Value, generated: Value) -> Result<()> {
    let existing = object_mut(existing, "existing Devin config root")?;
    let Value::Object(generated) = generated else {
        bail!("generated Devin config root must be a JSON object");
    };
    existing.extend(generated);
    Ok(())
}

fn object_mut<'a>(value: &'a mut Value, name: &str) -> Result<&'a mut Map<String, Value>> {
    match value {
        Value::Object(object) => Ok(object),
        _ => bail!("{name} must be a JSON object"),
    }
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
            read_json(&target)?,
            json!({
                "version": 1,
                "devin": {"org_id": "org-123"},
                "agent": {"model": "swe-2-max"},
                "hooks": {"SessionStart": ["generated-hook"]}
            }),
        );
        std::fs::remove_dir_all(root)?;
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

        assert_eq!(read_json(&target)?, generated);
        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    fn test_root(name: &str) -> Result<PathBuf> {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let root = std::env::temp_dir().join(format!("agent-harness-{name}-{nanos}"));
        std::fs::create_dir_all(&root)?;
        Ok(root)
    }

    fn write_file(path: &Path, content: &str) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, content)?;
        Ok(())
    }
}
