use std::path::Path;

use anyhow::{Context, Result};
use serde_json::{Map, Value};

use crate::file_op::FileOp;

const MANAGED_KEYS: &[&str] = &["defaultProvider", "defaultModel", "defaultThinkingLevel"];

pub(crate) fn managed_config_op(source: &Path, target: &Path) -> Result<FileOp> {
    sync_config_op(&source.join("pi/settings.json"), target)
}

pub(crate) fn sync_config_op(source: &Path, target: &Path) -> Result<FileOp> {
    let generated = managed_settings(read_settings(source)?);
    let mut existing = read_settings(target)?;
    existing.extend(generated);
    FileOp::write_json(target, &existing)
}

fn managed_settings(mut settings: Map<String, Value>) -> Map<String, Value> {
    settings.retain(|key, _| MANAGED_KEYS.contains(&key.as_str()));
    settings
}

fn read_settings(path: &Path) -> Result<Map<String, Value>> {
    match std::fs::read_to_string(path) {
        Ok(content) => serde_json::from_str(&content)
            .with_context(|| format!("failed to parse Pi settings {}", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Map::new()),
        Err(error) => {
            Err(error).with_context(|| format!("failed to read Pi settings {}", path.display()))
        }
    }
}
