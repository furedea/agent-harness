#![forbid(clippy::disallowed_methods, clippy::disallowed_types)]

use std::collections::BTreeSet;
use std::path::PathBuf;

use crate::{file_op::FileOp, layout::InstalledLayout, source_model::Source};

/// The operations of an installation, computed up front and applied in stages.
///
/// A stage holds operations whose targets do not overlap; a later stage may write inside a
/// directory an earlier one replaced, so the stages are applied in order, each all-or-nothing. The
/// whole plan is a function of the `Source` and the installed configs, with nothing read between
/// stages.
#[derive(Debug, Default)]
pub(crate) struct InstallPlan {
    stages: Vec<Vec<FileOp>>,
}

impl InstallPlan {
    pub(crate) fn push_stage(&mut self, operations: Vec<FileOp>) {
        self.stages.push(operations);
    }

    pub(crate) fn stages(&self) -> &[Vec<FileOp>] {
        &self.stages
    }

    /// The files the stages so far leave in the output, to refuse an asset that lands on one.
    pub(crate) fn planned_files(&self, source: &Source) -> BTreeSet<PathBuf> {
        let mut files = BTreeSet::new();
        self.stages
            .iter()
            .flatten()
            .for_each(|operation| collect_files(operation, source, &mut files));
        files
    }
}

fn collect_files(operation: &FileOp, source: &Source, files: &mut BTreeSet<PathBuf>) {
    match operation {
        FileOp::WriteFile { target, .. } | FileOp::CopyFile { target, .. } => {
            files.insert(target.clone());
        }
        FileOp::CopyTree {
            source: directory,
            target,
        } => {
            let listed = source.directory(directory).into_iter();
            files.extend(listed.flat_map(|dir| dir.relative_files().map(|path| target.join(path))));
        }
        FileOp::ReplaceTree { contents, .. } => contents
            .iter()
            .for_each(|content| collect_files(content, source, files)),
    }
}

/// Plan publication of a complete rendered installation without accessing the filesystem.
pub(crate) fn plan_install(
    rendered: &InstalledLayout<'_>,
    installed: &InstalledLayout<'_>,
) -> Vec<FileOp> {
    plan_install_for(rendered, installed, crate::provider::spec::PROVIDERS)
}

pub(crate) fn plan_install_for(
    rendered: &InstalledLayout<'_>,
    installed: &InstalledLayout<'_>,
    providers: &[crate::provider::spec::ProviderSpec],
) -> Vec<FileOp> {
    let directories = rendered
        .managed_directories_for(providers)
        .into_iter()
        .zip(installed.managed_directories_for(providers))
        .map(|(source, target)| FileOp::CopyTree { source, target });
    let files = rendered
        .managed_files_for(providers)
        .into_iter()
        .zip(installed.managed_files_for(providers))
        .map(|(source, target)| FileOp::CopyFile { source, target });
    directories.chain(files).collect()
}

#[cfg(test)]
mod tests;

pub(crate) fn plan_assets_for(
    out: &std::path::Path,
    providers: &[crate::provider::spec::ProviderSpec],
) -> Vec<FileOp> {
    providers
        .iter()
        .flat_map(|spec| spec.hook.bridge.assets.iter())
        .map(|asset| FileOp::write(out.join(asset.destination), asset.content))
        .collect()
}
