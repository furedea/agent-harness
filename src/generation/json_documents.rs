use std::path::Path;

use anyhow::{Context, Result};
use serde_json::{Map, Value};

use crate::snapshot::Snapshot;

/// The JSON document at `path`.
pub(super) fn read_json(files: &Snapshot, path: &Path) -> Result<Value> {
    let content = files
        .read_text(path)
        .with_context(|| format!("failed to read JSON file {}", path.display()))?;
    parse_json(content, path)
}

/// The JSON document at `path`, or an empty object when the file does not exist.
pub(super) fn read_json_or_empty(files: &Snapshot, path: &Path) -> Result<Value> {
    match files.read_text_if_present(path)? {
        Some(content) => parse_json(content, path),
        None => Ok(Value::Object(Map::new())),
    }
}

fn parse_json(content: &str, path: &Path) -> Result<Value> {
    serde_json::from_str(content)
        .with_context(|| format!("failed to parse JSON file {}", path.display()))
}
