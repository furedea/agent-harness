use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

use super::{execute, load_snapshot, real_path, sync_documents};
use crate::{
    file_op::FileOp,
    generation::{claude_config, codex_config, devin_config, dsh_config, hermes_config, pi_config},
    provider::spec::{ConfigSync, ProviderSpec},
    snapshot::{ConfigFiles, SnapshotRequest},
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
    let source = rendered_source(sync, paths.source)?;
    let mut request = SnapshotRequest::default();
    request.documents.extend(sync_documents(sync, paths.target));
    request.documents.push(source.clone());
    let files = load_snapshot(&request)?;
    let config = ConfigFiles {
        generated: &source,
        target: paths.target,
        files: &files,
    };
    match sync {
        ConfigSync::ReplaceGeneratedKeys => {
            claude_config::sync_settings_op(config, home).map(|operation| vec![operation])
        }
        ConfigSync::MergeToml => codex_config::sync_managed_config_ops(config),
        ConfigSync::ReplaceListedKeys => {
            pi_config::sync_config_op(config).map(|operation| vec![operation])
        }
        ConfigSync::MergePermissionEntries => {
            devin_config::sync_config_op(config).map(|operation| vec![operation])
        }
        ConfigSync::MergeYaml => {
            hermes_config::sync_config_op(config).map(|operation| vec![operation])
        }
        ConfigSync::RegisterBundle => dsh_config::sync_config_ops(config),
    }
}

/// The rendered input of a sync: the bridge bundle is a directory whose real path is registered.
fn rendered_source(sync: ConfigSync, source: &Path) -> Result<PathBuf> {
    if sync != ConfigSync::RegisterBundle {
        return Ok(source.to_path_buf());
    }
    real_path(source)
}

#[cfg(test)]
mod tests;
