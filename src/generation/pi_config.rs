use std::path::Path;

use anyhow::{Context, Result};
use serde_json::{Map, Value};

use crate::fs_ops;

const MANAGED_KEYS: &[&str] = &["defaultProvider", "defaultModel", "defaultThinkingLevel"];

pub(crate) fn write_managed_config(source: &Path, target: &Path) -> Result<()> {
    sync_config(&source.join("pi/settings.json"), target)
}

pub(crate) fn sync_config(source: &Path, target: &Path) -> Result<()> {
    let generated = managed_settings(read_settings(source)?);
    let mut existing = read_settings(target)?;
    existing.extend(generated);
    let content = serde_json::to_string_pretty(&existing)? + "\n";
    fs_ops::write_file_atomically(target, content.as_bytes())
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
