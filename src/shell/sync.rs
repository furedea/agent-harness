use std::path::Path;

use anyhow::{Result, bail};

use super::execute;
use crate::{
    file_op::FileOp,
    generation::{claude_config, codex_config, devin_config, dsh_config, hermes_config, pi_config},
    provider::spec::{ConfigSync, ProviderSpec},
};

/// The rendered config to read and the user-owned config to update.
pub(crate) struct SyncPaths<'a> {
    pub(crate) source: &'a Path,
    pub(crate) target: &'a Path,
}

pub(crate) fn sync(spec: &ProviderSpec, paths: &SyncPaths<'_>, home: &Path) -> Result<()> {
    let Some(config) = spec.config else {
        bail!("{} has no managed config to sync", spec.key);
    };
    execute(&sync_ops(config.sync, paths, home)?)
}

fn sync_ops(sync: ConfigSync, paths: &SyncPaths<'_>, home: &Path) -> Result<Vec<FileOp>> {
    let SyncPaths { source, target } = *paths;
    match sync {
        ConfigSync::ReplaceGeneratedKeys => {
            claude_config::sync_settings_op(source, target, home).map(|operation| vec![operation])
        }
        ConfigSync::MergeToml => codex_config::sync_managed_config_ops(source, target),
        ConfigSync::ReplaceListedKeys => {
            pi_config::sync_config_op(source, target).map(|operation| vec![operation])
        }
        ConfigSync::MergePermissionEntries => {
            devin_config::sync_config_op(source, target).map(|operation| vec![operation])
        }
        ConfigSync::MergeYaml => {
            hermes_config::sync_config_op(source, target).map(|operation| vec![operation])
        }
        ConfigSync::RegisterBundle => dsh_config::sync_config_ops(source, target),
    }
}
