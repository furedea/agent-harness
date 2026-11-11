use std::path::Path;

use anyhow::{Result, bail};

use super::{execute, load_snapshot, real_path, sync_documents};
use crate::{
    config_sync::{
        Merge,
        codec::ConfigDocument,
        installed::{Installed, read_generated},
    },
    file_op::FileOp,
    generation::dsh_config,
    provider::spec::{ConfigLocation, ConfigSpec, ProviderSpec},
    snapshot::{Snapshot, SnapshotRequest},
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
    let Some(operations) = sync_ops(config, paths, home)? else {
        eprintln!(
            "Skipping {} config {}: run {} once to initialize it, then reactivate.",
            spec.key,
            paths.target.display(),
            spec.key
        );
        return Ok(());
    };
    execute(&operations)
}

/// Reads the rendered and installed configs once, parses them, and applies the declared ownership.
///
/// Returns `None` when the provider has not created its config yet and the sync leaves it alone.
fn sync_ops(config: ConfigSpec, paths: &SyncPaths<'_>, home: &Path) -> Result<Option<Vec<FileOp>>> {
    let sync = &config.sync;
    let rendered = rendered_input(config.location, paths.source)?;
    let mut request = SnapshotRequest::default();
    request.documents.extend(sync_documents(sync, paths.target));
    request
        .documents
        .extend(rendered.document().map(Path::to_path_buf));
    let files = load_snapshot(&request)?;
    let generated = rendered.parse(&config, &files)?;
    let installed = Installed::from_snapshot(sync, &files, paths.target)?;
    let merge = Merge::new(sync, &generated).expanding_home(home);
    if merge.skips(&installed) {
        return Ok(None);
    }
    merge
        .apply(installed)?
        .into_ops(sync, paths.target)
        .map(Some)
}

/// The rendered input of a sync.
enum Rendered<'a> {
    /// A config file in the provider's format.
    File(&'a Path),
    /// A bundle directory whose real path the installed config registers.
    Bundle(std::path::PathBuf),
}

impl Rendered<'_> {
    fn document(&self) -> Option<&Path> {
        match self {
            Self::File(path) => Some(path),
            Self::Bundle(_) => None,
        }
    }

    fn parse(&self, config: &ConfigSpec, files: &Snapshot) -> Result<ConfigDocument> {
        match self {
            Self::File(path) => read_generated(&config.sync, files, path),
            Self::Bundle(directory) => dsh_config::bundle_registration(directory),
        }
    }
}

fn rendered_input(location: ConfigLocation, source: &Path) -> Result<Rendered<'_>> {
    match location {
        ConfigLocation::BundleDirectory(_) => real_path(source).map(Rendered::Bundle),
        ConfigLocation::SameAsHookRegistration | ConfigLocation::File(_) => {
            Ok(Rendered::File(source))
        }
    }
}

#[cfg(test)]
mod tests;
