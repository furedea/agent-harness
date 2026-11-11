use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::{filesystem::FileSystem, install_plan::FileOp};

/// A replacement prepared next to the path it will replace.
struct Staged<'a> {
    target: &'a Path,
    path: PathBuf,
}

/// A path that now holds its replacement and the original it can be restored from.
struct Swapped<'a> {
    target: &'a Path,
    backup: Option<PathBuf>,
}

/// Applies the plan all-or-nothing.
///
/// Every replacement is staged before any live path is touched, so a failure while preparing
/// changes nothing. The swaps that follow are renames, and a failing one restores every path
/// already swapped.
pub(crate) fn execute(filesystem: &impl FileSystem, plan: &[FileOp]) -> Result<()> {
    let staged = stage_all(filesystem, plan)?;
    let mut swapped = Vec::new();
    let result = staged
        .iter()
        .try_for_each(|replacement| swap(filesystem, replacement, &mut swapped));
    match result {
        Ok(()) => {
            commit(filesystem, &swapped);
            Ok(())
        }
        Err(error) => {
            discard_staged(filesystem, &staged);
            Err(roll_back(filesystem, &swapped, error))
        }
    }
}

fn stage_all<'a>(filesystem: &impl FileSystem, plan: &'a [FileOp]) -> Result<Vec<Staged<'a>>> {
    let mut staged = Vec::new();
    let result = plan.iter().try_for_each(|operation| {
        staged.push(stage(filesystem, operation)?);
        Ok(())
    });
    if let Err(error) = result {
        discard_staged(filesystem, &staged);
        return Err(error);
    }
    Ok(staged)
}

fn stage<'a>(filesystem: &impl FileSystem, operation: &'a FileOp) -> Result<Staged<'a>> {
    let target = operation.target();
    let path = match operation {
        FileOp::WriteFile { content, .. } => filesystem.stage_file(target, content),
        FileOp::ReplaceDirectory { source, .. } => filesystem.stage_directory(source, target),
        FileOp::InstallFile { source, .. } => {
            let content = filesystem
                .read_file(source)
                .with_context(|| format!("failed to read {}", source.display()))?;
            filesystem.stage_file(target, &content)
        }
    }?;
    Ok(Staged { target, path })
}

fn swap<'a>(
    filesystem: &impl FileSystem,
    replacement: &Staged<'a>,
    swapped: &mut Vec<Swapped<'a>>,
) -> Result<()> {
    let backup = filesystem.swap(&replacement.path, replacement.target)?;
    swapped.push(Swapped {
        target: replacement.target,
        backup,
    });
    Ok(())
}

/// Backups are leftovers once every swap succeeded, so failing to delete one is not a failure.
fn commit(filesystem: &impl FileSystem, swapped: &[Swapped<'_>]) {
    for backup in swapped
        .iter()
        .filter_map(|published| published.backup.as_deref())
    {
        let _ = filesystem.discard(backup);
    }
}

fn discard_staged(filesystem: &impl FileSystem, staged: &[Staged<'_>]) {
    for replacement in staged {
        let _ = filesystem.discard(&replacement.path);
    }
}

/// Restores every swapped path, newest first, and reports every restore that failed.
fn roll_back(
    filesystem: &impl FileSystem,
    swapped: &[Swapped<'_>],
    cause: anyhow::Error,
) -> anyhow::Error {
    let failures: Vec<String> = swapped
        .iter()
        .rev()
        .filter_map(|published| {
            filesystem
                .restore(published.target, published.backup.as_deref())
                .err()
        })
        .map(|error| format!("{error:#}"))
        .collect();
    if failures.is_empty() {
        return cause.context("install failed; previous files were restored");
    }
    cause.context(format!("rollback also failed: {}", failures.join("; ")))
}
