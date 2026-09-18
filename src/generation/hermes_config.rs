use std::path::Path;

use anyhow::{Context, Result, bail};
use serde_norway::{Mapping, Value};

use crate::fs_ops;

const MANAGED_CONFIG: &str = "plugins:\n  enabled:\n    - agent-harness-hooks\n";

pub(crate) fn write_managed_config(path: &Path) -> Result<()> {
    fs_ops::write_file_atomically(path, MANAGED_CONFIG.as_bytes())
}

pub(crate) fn sync_config(source_path: &Path, target_path: &Path) -> Result<()> {
    let generated = read_yaml(source_path)?;
    let mut existing = read_yaml_or_empty(target_path)?;
    merge_managed_config(&mut existing, generated)?;
    let content = serde_norway::to_string(&existing)?;
    fs_ops::write_file_atomically(target_path, content.as_bytes())
}

fn merge_managed_config(existing: &mut Value, generated: Value) -> Result<()> {
    let existing = mapping_mut(existing, "existing Hermes config root")?;
    let Value::Mapping(generated) = generated else {
        bail!("generated Hermes config root must be a mapping");
    };
    for (key, value) in generated {
        if key.as_str() == Some("plugins") {
            let entry = existing
                .entry(key)
                .or_insert_with(|| Value::Mapping(Mapping::new()));
            merge_plugins(entry, value)?;
        } else {
            existing.insert(key, value);
        }
    }
    Ok(())
}

fn merge_plugins(existing: &mut Value, generated: Value) -> Result<()> {
    let existing = mapping_mut(existing, "existing plugins section")?;
    let Value::Mapping(generated) = generated else {
        bail!("generated plugins section must be a mapping");
    };
    for (key, value) in generated {
        if key.as_str() == Some("enabled") {
            let entry = existing
                .entry(key)
                .or_insert_with(|| Value::Sequence(Vec::new()));
            merge_enabled(entry, value)?;
        } else {
            existing.insert(key, value);
        }
    }
    Ok(())
}

fn merge_enabled(existing: &mut Value, generated: Value) -> Result<()> {
    let Value::Sequence(existing) = existing else {
        bail!("existing plugins.enabled must be a sequence");
    };
    let Value::Sequence(generated) = generated else {
        bail!("generated plugins.enabled must be a sequence");
    };
    for item in generated {
        if !existing.contains(&item) {
            existing.push(item);
        }
    }
    Ok(())
}

fn mapping_mut<'a>(value: &'a mut Value, name: &str) -> Result<&'a mut Mapping> {
    match value {
        Value::Mapping(mapping) => Ok(mapping),
        _ => bail!("{name} must be a mapping"),
    }
}

fn read_yaml(path: &Path) -> Result<Value> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read YAML file {}", path.display()))?;
    serde_norway::from_str(&content)
        .with_context(|| format!("failed to parse YAML file {}", path.display()))
}

fn read_yaml_or_empty(path: &Path) -> Result<Value> {
    match std::fs::read_to_string(path) {
        Ok(content) => {
            let value: Value = serde_norway::from_str(&content)
                .with_context(|| format!("failed to parse YAML file {}", path.display()))?;
            Ok(match value {
                Value::Null => Value::Mapping(Mapping::new()),
                value => value,
            })
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(Value::Mapping(Mapping::new()))
        }
        Err(error) => {
            Err(error).with_context(|| format!("failed to read YAML file {}", path.display()))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn sync_config_unions_plugins_enabled_and_preserves_user_keys() -> Result<()> {
        let root = test_root("sync-hermes-config")?;
        let source = root.join("managed.yaml");
        let target = root.join("config.yaml");
        write_file(&source, "plugins:\n  enabled:\n    - agent-harness-hooks\n")?;
        write_file(
            &target,
            "model:\n  provider: openai-codex\n  default: gpt-5.6\nplugins:\n  enabled:\n    - moshi-hooks\nonboarding:\n  seen:\n    intro: true\n",
        )?;

        sync_config(&source, &target)?;

        let parsed: Value = serde_norway::from_str(&std::fs::read_to_string(&target)?)?;
        assert_eq!(parsed["model"]["provider"].as_str(), Some("openai-codex"));
        assert_eq!(
            parsed["plugins"]["enabled"],
            serde_norway::from_str::<Value>("- moshi-hooks\n- agent-harness-hooks\n")?,
        );
        assert_eq!(parsed["onboarding"]["seen"]["intro"].as_bool(), Some(true));
        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn sync_config_dedupes_and_creates_a_missing_target() -> Result<()> {
        let root = test_root("sync-hermes-config-missing")?;
        let source = root.join("managed.yaml");
        let target = root.join("nested/config.yaml");
        write_file(&source, "plugins:\n  enabled:\n    - agent-harness-hooks\n")?;

        sync_config(&source, &target)?;
        sync_config(&source, &target)?;

        let parsed: Value = serde_norway::from_str(&std::fs::read_to_string(&target)?)?;
        assert_eq!(
            parsed["plugins"]["enabled"].as_sequence().map(Vec::len),
            Some(1),
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

    fn write_file(path: &Path, content: &str) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, content)?;
        Ok(())
    }
}
