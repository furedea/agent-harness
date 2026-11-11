use std::path::Path;

use anyhow::{Context, Result, bail};
use serde_norway::{Mapping, Value};

use crate::{
    file_op::FileOp,
    snapshot::{ConfigFiles, Snapshot},
};

const MANAGED_CONFIG: &str = "plugins:\n  enabled:\n    - agent-harness-hooks\n";

pub(crate) fn managed_config_op(path: &Path) -> FileOp {
    FileOp::write(path, MANAGED_CONFIG)
}

pub(crate) fn sync_config_op(config: ConfigFiles<'_>) -> Result<FileOp> {
    let generated = read_yaml(config.files, config.generated)?;
    let mut existing = read_yaml_or_empty(config.files, config.target)?;
    merge_managed_config(&mut existing, generated)?;
    Ok(FileOp::write(
        config.target,
        serde_norway::to_string(&existing)?,
    ))
}

fn merge_managed_config(existing: &mut Value, generated: Value) -> Result<()> {
    let existing = mapping_mut(existing, "existing Hermes config root")?;
    let Value::Mapping(generated) = generated else {
        bail!("generated Hermes config root must be a mapping");
    };
    for (key, value) in generated {
        merge_root_entry(existing, key, value)?;
    }
    Ok(())
}

fn merge_root_entry(existing: &mut Mapping, key: Value, value: Value) -> Result<()> {
    if key.as_str() != Some("plugins") {
        existing.insert(key, value);
        return Ok(());
    }
    let entry = existing
        .entry(key)
        .or_insert_with(|| Value::Mapping(Mapping::new()));
    merge_plugins(entry, value)
}

fn merge_plugins(existing: &mut Value, generated: Value) -> Result<()> {
    let existing = mapping_mut(existing, "existing plugins section")?;
    let Value::Mapping(generated) = generated else {
        bail!("generated plugins section must be a mapping");
    };
    for (key, value) in generated {
        merge_plugin_entry(existing, key, value)?;
    }
    Ok(())
}

fn merge_plugin_entry(existing: &mut Mapping, key: Value, value: Value) -> Result<()> {
    if key.as_str() != Some("enabled") {
        existing.insert(key, value);
        return Ok(());
    }
    let entry = existing
        .entry(key)
        .or_insert_with(|| Value::Sequence(Vec::new()));
    merge_enabled(entry, value)
}

fn merge_enabled(existing: &mut Value, generated: Value) -> Result<()> {
    let Value::Sequence(existing) = existing else {
        bail!("existing plugins.enabled must be a sequence");
    };
    let Value::Sequence(generated) = generated else {
        bail!("generated plugins.enabled must be a sequence");
    };
    for item in generated {
        push_unique(existing, item);
    }
    Ok(())
}

fn push_unique(items: &mut Vec<Value>, item: Value) {
    if !items.contains(&item) {
        items.push(item);
    }
}

fn mapping_mut<'a>(value: &'a mut Value, name: &str) -> Result<&'a mut Mapping> {
    match value {
        Value::Mapping(mapping) => Ok(mapping),
        _ => bail!("{name} must be a mapping"),
    }
}

fn read_yaml(files: &Snapshot, path: &Path) -> Result<Value> {
    let content = files
        .read_text(path)
        .with_context(|| format!("failed to read YAML file {}", path.display()))?;
    serde_norway::from_str(content)
        .with_context(|| format!("failed to parse YAML file {}", path.display()))
}

fn read_yaml_or_empty(files: &Snapshot, path: &Path) -> Result<Value> {
    let Some(content) = files.read_text_if_present(path)? else {
        return Ok(Value::Mapping(Mapping::new()));
    };
    let value: Value = serde_norway::from_str(content)
        .with_context(|| format!("failed to parse YAML file {}", path.display()))?;
    Ok(match value {
        Value::Null => Value::Mapping(Mapping::new()),
        value => value,
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use crate::shell::fixtures::load_documents;

    fn sync_config(source: &Path, target: &Path) -> Result<()> {
        let files = load_documents(&[source, target])?;
        let config = ConfigFiles {
            generated: source,
            target,
            files: &files,
        };
        crate::shell::execute(&[sync_config_op(config)?])
    }

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
        crate::shell::fixtures::remove_dir_all(root)?;
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
