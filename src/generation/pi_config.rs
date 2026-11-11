use std::path::Path;

use anyhow::{Context, Result};
use serde_json::{Map, Value};

use crate::{
    file_op::FileOp,
    snapshot::{ConfigFiles, Snapshot, Tree},
};

const MANAGED_KEYS: &[&str] = &["defaultProvider", "defaultModel", "defaultThinkingLevel"];

pub(crate) fn managed_config_op(source: Tree<'_>, target: &Path) -> Result<FileOp> {
    sync_config_op(ConfigFiles {
        generated: &source.root().join("pi/settings.json"),
        target,
        files: source.files(),
    })
}

pub(crate) fn sync_config_op(config: ConfigFiles<'_>) -> Result<FileOp> {
    let generated = managed_settings(read_settings(config.files, config.generated)?);
    let mut existing = read_settings(config.files, config.target)?;
    existing.extend(generated);
    FileOp::write_json(config.target, &existing)
}

fn managed_settings(mut settings: Map<String, Value>) -> Map<String, Value> {
    settings.retain(|key, _| MANAGED_KEYS.contains(&key.as_str()));
    settings
}

fn read_settings(files: &Snapshot, path: &Path) -> Result<Map<String, Value>> {
    let Some(content) = files.read_text_if_present(path)? else {
        return Ok(Map::new());
    };
    serde_json::from_str(content)
        .with_context(|| format!("failed to parse Pi settings {}", path.display()))
}
