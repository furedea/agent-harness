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

/// Everything an execution changed on disk before its swaps are final.
struct Transaction<'a, F: FileSystem> {
    filesystem: &'a F,
    created_parents: Vec<PathBuf>,
    staged: Vec<Staged<'a>>,
    swapped: Vec<Swapped<'a>>,
}

/// Applies the plan all-or-nothing.
///
/// Every replacement is staged before any live path is touched, so a failure while preparing
/// changes nothing. The swaps that follow are renames, and a failing one restores every path
/// already swapped. Directories created for the targets are removed again on failure.
pub(crate) fn execute(filesystem: &impl FileSystem, plan: &[FileOp]) -> Result<()> {
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
    let created = transaction.filesystem.create_parents(target)?;
    transaction.created_parents.extend(created);
    let path = stage_path(transaction.filesystem, operation)?;
    transaction.staged.push(Staged { target, path });
    Ok(())
}

fn stage_path(filesystem: &impl FileSystem, operation: &FileOp) -> Result<PathBuf> {
    match operation {
        FileOp::WriteFile { target, content } => filesystem.stage_file(target, content),
        FileOp::ReplaceDirectory { source, target } => filesystem.stage_directory(source, target),
        FileOp::InstallFile { source, target } => {
            let content = filesystem
                .read_file(source)
                .with_context(|| format!("failed to read {}", source.display()))?;
            filesystem.stage_file(target, &content)
        }
    }
}

fn swap<F: FileSystem>(transaction: &mut Transaction<'_, F>, index: usize) -> Result<()> {
    let Some(replacement) = transaction.staged.get(index) else {
        return Ok(());
    };
    let target = replacement.target;
    let backup = transaction.filesystem.swap(&replacement.path, target)?;
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
fn abort<F: FileSystem>(transaction: &Transaction<'_, F>, cause: anyhow::Error) -> anyhow::Error {
    for replacement in &transaction.staged {
        let _ = transaction.filesystem.discard(&replacement.path);
    }
    let failures = restore_failures(transaction);
    if !failures.is_empty() {
        return cause.context(format!("rollback also failed: {}", failures.join("; ")));
    }
    transaction
        .filesystem
        .remove_created(&transaction.created_parents);
    cause.context("install failed; previous files were restored")
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

#[cfg(all(test, unix))]
mod tests;
