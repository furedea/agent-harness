use std::path::Path;

use anyhow::{Context, Result};
use serde::Serialize;

pub(super) fn write_file(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory {}", parent.display()))?;
    }
    std::fs::write(path, content).with_context(|| format!("failed to write {}", path.display()))
}

pub(super) fn write_json<T>(path: &Path, value: &T) -> Result<()>
where
    T: Serialize + ?Sized,
{
    let content = serde_json::to_string_pretty(value)? + "\n";
    write_file(path, &content)
}
