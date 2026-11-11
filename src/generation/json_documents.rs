use std::path::Path;

use anyhow::{Context, Result};
use serde_json::Value;

use crate::snapshot::Snapshot;

/// The JSON document at `path`.
pub(super) fn read_json(files: &Snapshot, path: &Path) -> Result<Value> {
    let content = files
        .read_text(path)
        .with_context(|| format!("failed to read JSON file {}", path.display()))?;
    serde_json::from_str(content)
        .with_context(|| format!("failed to parse JSON file {}", path.display()))
}
