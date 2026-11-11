use std::path::Path;

use anyhow::{Result, bail};

use crate::{
    generation::{claude_config, codex_config, devin_config, dsh_config, hermes_config, pi_config},
    provider::spec::{ConfigSync, ProviderSpec},
};

/// The rendered config to read and the user-owned config to update.
pub(crate) struct SyncPaths<'a> {
    pub(crate) source: &'a Path,
    pub(crate) target: &'a Path,
}

pub(crate) fn sync(spec: &ProviderSpec, paths: &SyncPaths<'_>, home: &Path) -> Result<()> {
    let SyncPaths { source, target } = *paths;
    let Some(config) = spec.config else {
        bail!("{} has no managed config to sync", spec.key);
    };
    match config.sync {
        ConfigSync::ReplaceGeneratedKeys => claude_config::sync_settings(source, target, home),
        ConfigSync::MergeToml => codex_config::sync_managed_config(source, target),
        ConfigSync::ReplaceListedKeys => pi_config::sync_config(source, target),
        ConfigSync::MergePermissionEntries => devin_config::sync_config(source, target),
        ConfigSync::MergeYaml => hermes_config::sync_config(source, target),
        ConfigSync::RegisterBundle => dsh_config::sync_config(source, target),
    }
}
