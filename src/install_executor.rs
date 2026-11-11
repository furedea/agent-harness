use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::{filesystem::FileSystem, install_plan::FileOp};

/// A path an operation is about to replace and the original it can be restored from.
struct Publication<'a> {
    target: &'a Path,
    backup: Option<PathBuf>,
}

/// Applies the plan all-or-nothing: a failing operation restores every path already touched.
pub(crate) fn execute(filesystem: &impl FileSystem, plan: &[FileOp]) -> Result<()> {
    let mut publications = Vec::new();
    let published = plan
        .iter()
        .try_for_each(|operation| publish(filesystem, operation, &mut publications));
    match published {
        Ok(()) => commit(filesystem, &publications),
        Err(error) => Err(roll_back(filesystem, &publications, error)),
    }
}

fn commit(filesystem: &impl FileSystem, publications: &[Publication<'_>]) -> Result<()> {
    publications
        .iter()
        .filter_map(|publication| publication.backup.as_deref())
        .try_for_each(|backup| filesystem.discard(backup))
}

fn publish<'a>(
    filesystem: &impl FileSystem,
    operation: &'a FileOp,
    publications: &mut Vec<Publication<'a>>,
) -> Result<()> {
    let target = operation.target();
    let backup = filesystem.move_aside(target)?;
    publications.push(Publication { target, backup });
    apply(filesystem, operation)
}

fn roll_back(
    filesystem: &impl FileSystem,
    publications: &[Publication<'_>],
    cause: anyhow::Error,
) -> anyhow::Error {
    let failure = publications.iter().rev().find_map(|publication| {
        filesystem
            .restore(publication.target, publication.backup.as_deref())
            .err()
    });
    match failure {
        Some(restore_error) => cause.context(format!("rollback also failed: {restore_error:#}")),
        None => cause.context("install failed; previous files were restored"),
    }
}

fn apply(filesystem: &impl FileSystem, operation: &FileOp) -> Result<()> {
    match operation {
        FileOp::WriteFile { target, content } => filesystem.write_file(target, content),
        FileOp::ReplaceDirectory { source, target } => filesystem.replace_directory(source, target),
        FileOp::InstallFile { source, target } => {
            let content = filesystem
                .read_file(source)
                .with_context(|| format!("failed to read {}", source.display()))?;
            filesystem.write_file(target, &content)
        }
    }
}
