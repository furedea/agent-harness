#![forbid(clippy::disallowed_methods, clippy::disallowed_types)]

use anyhow::{Context, Result, ensure};
use serde::Deserialize;

use crate::snapshot::Tree;

const SUPPORTED_VERSION: u64 = 1;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Manifest {
    version: u64,
    runtime_commands: Vec<String>,
}

impl Manifest {
    pub(crate) fn read(source: Tree<'_>) -> Result<Self> {
        let path = source.root().join("manifest.json");
        let content = source
            .files()
            .read_text(&path)
            .with_context(|| format!("failed to read source manifest {}", path.display()))?;
        let manifest = serde_json::from_str::<Self>(content)
            .with_context(|| format!("failed to parse source manifest {}", path.display()))?;
        ensure!(
            manifest.version == SUPPORTED_VERSION,
            "unsupported source manifest version {} in {}",
            manifest.version,
            path.display(),
        );
        Ok(manifest)
    }

    pub(crate) fn runtime_commands(&self) -> &[String] {
        &self.runtime_commands
    }
}
