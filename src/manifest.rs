use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

const SUPPORTED_VERSION: u64 = 1;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Manifest {
    version: u64,
    runtime_commands: Vec<String>,
}

impl Manifest {
    pub(crate) fn read(source: &Path) -> Result<Self> {
        let path = source.join("manifest.json");
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read source manifest {}", path.display()))?;
        let manifest = serde_json::from_str::<Self>(&content)
            .with_context(|| format!("failed to parse source manifest {}", path.display()))?;
        if manifest.version != SUPPORTED_VERSION {
            bail!(
                "unsupported source manifest version {} in {}",
                manifest.version,
                path.display(),
            );
        }
        Ok(manifest)
    }

    pub(crate) fn runtime_commands(&self) -> &[String] {
        &self.runtime_commands
    }
}
