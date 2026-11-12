#![forbid(clippy::disallowed_methods, clippy::disallowed_types)]

use std::path::Path;

use anyhow::{Context, Result, ensure};
use serde::Deserialize;

const SUPPORTED_VERSION: u64 = 1;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Manifest {
    version: u64,
    runtime_commands: Vec<String>,
}

impl Manifest {
    /// Parses and validates the manifest `text` read from `path`.
    ///
    /// # Errors
    ///
    /// Returns an error naming `path` when the text is not a supported manifest.
    pub(crate) fn parse(path: &Path, text: &str) -> Result<Self> {
        let manifest = serde_json::from_str::<Self>(text)
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
