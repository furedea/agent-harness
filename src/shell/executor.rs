use std::path::{Path, PathBuf};

use anyhow::Result;

use super::io::{FileSystem, NewFile, RealFileSystem};
use crate::{file_op::FileOp, install_plan::InstallPlan};

/// A replacement prepared next to the path it will replace, or `None` when the path is removed.
struct Staged<'a> {
    target: &'a Path,
    path: Option<PathBuf>,
}

/// A path that now holds its replacement and the original it can be restored from.
struct Swapped<'a> {
    target: &'a Path,
    backup: Option<PathBuf>,
}

/// Everything an execution changed on disk before its swaps are final.
struct Transaction<'a, F: FileSystem> {
    filesystem: &'a F,
    created_parents: Vec<PathBuf>,
    staged: Vec<Staged<'a>>,
    swapped: Vec<Swapped<'a>>,
}

/// Applies each stage of an installation in order, each all-or-nothing.
pub(crate) fn execute_stages(plan: &InstallPlan) -> Result<()> {
    plan.stages().iter().try_for_each(|stage| execute(stage))
}

/// Applies the plan on the real filesystem, all-or-nothing. This is the only way to write.
pub(crate) fn execute(plan: &[FileOp]) -> Result<()> {
    execute_on(&RealFileSystem, plan)
}

/// Applies the plan all-or-nothing.
///
/// Every replacement is staged before any live path is touched, so a failure while preparing
/// changes nothing. The swaps that follow are renames, and a failing one restores every path
/// already swapped. Directories created for the targets are removed again on failure.
fn execute_on(filesystem: &impl FileSystem, plan: &[FileOp]) -> Result<()> {
    let mut transaction = Transaction {
        filesystem,
        created_parents: Vec::new(),
        staged: Vec::new(),
        swapped: Vec::new(),
    };
    match run(&mut transaction, plan) {
        Ok(()) => {
            discard_backups(&transaction);
            Ok(())
        }
        Err(error) => Err(abort(&transaction, error)),
    }
}

fn run<'a, F: FileSystem>(transaction: &mut Transaction<'a, F>, plan: &'a [FileOp]) -> Result<()> {
    for operation in plan {
        stage(transaction, operation)?;
    }
    for index in 0..transaction.staged.len() {
        swap(transaction, index)?;
    }
    Ok(())
}

fn stage<'a, F: FileSystem>(
    transaction: &mut Transaction<'a, F>,
    operation: &'a FileOp,
) -> Result<()> {
    let target = operation.target();
    if !matches!(operation, FileOp::RemoveFile { .. }) {
        let created = transaction.filesystem.create_parents(target)?;
        transaction.created_parents.extend(created);
    }
    let path = stage_path(transaction.filesystem, operation)?;
    transaction.staged.push(Staged { target, path });
    Ok(())
}

fn stage_path(filesystem: &impl FileSystem, operation: &FileOp) -> Result<Option<PathBuf>> {
    let path = match operation {
        FileOp::WriteFile {
            target,
            content,
            mode,
        } => filesystem.stage_file(
            target,
            &NewFile {
                content,
                mode: *mode,
            },
        ),
        FileOp::CopyFile { source, target } => filesystem.stage_copy(source, target),
        FileOp::CopyTree { source, target } => filesystem.stage_directory(source, target),
        FileOp::ReplaceTree { target, contents } => filesystem.stage_tree(target, contents),
        FileOp::RemoveFile { .. } => return Ok(None),
    }?;
    Ok(Some(path))
}

fn swap<F: FileSystem>(transaction: &mut Transaction<'_, F>, index: usize) -> Result<()> {
    let Some(replacement) = transaction.staged.get(index) else {
        return Ok(());
    };
    let target = replacement.target;
    let backup = match &replacement.path {
        Some(staged) => transaction.filesystem.swap(staged, target)?,
        None => transaction.filesystem.set_aside(target)?,
    };
    transaction.swapped.push(Swapped { target, backup });
    Ok(())
}

/// Backups are leftovers once every swap succeeded, so failing to delete one is harmless.
fn discard_backups<F: FileSystem>(transaction: &Transaction<'_, F>) {
    for swapped in &transaction.swapped {
        let _ = swapped
            .backup
            .as_deref()
            .map(|backup| transaction.filesystem.discard(backup));
    }
}

/// Restores every swapped path, newest first, and reports every restore that failed.
///
/// Directories this execution created are removed again only when they are empty, so they are
/// cleaned up even after a failed restore without touching anything that restore left behind.
fn abort<F: FileSystem>(transaction: &Transaction<'_, F>, cause: anyhow::Error) -> anyhow::Error {
    for staged in transaction
        .staged
        .iter()
        .filter_map(|staged| staged.path.as_deref())
    {
        let _ = transaction.filesystem.discard(staged);
    }
    let failures = restore_failures(transaction);
    transaction
        .filesystem
        .remove_created(&transaction.created_parents);
    if failures.is_empty() {
        return cause.context("install failed; previous files were restored");
    }
    cause.context(format!("rollback also failed: {}", failures.join("; ")))
}

fn restore_failures<F: FileSystem>(transaction: &Transaction<'_, F>) -> Vec<String> {
    let restore = |swapped: &Swapped<'_>| {
        transaction
            .filesystem
            .restore(swapped.target, swapped.backup.as_deref())
            .err()
    };
    transaction
        .swapped
        .iter()
        .rev()
        .filter_map(restore)
        .map(|error| format!("{error:#}"))
        .collect()
}

#[cfg(test)]
mod memory_tests;
#[cfg(all(test, unix))]
mod tests;
