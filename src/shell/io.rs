//! The filesystem port: the only code in the crate that writes to disk.
//!
//! # Contract
//!
//! - Nothing here is visible outside `shell`. Computation returns `FileOp` values and the
//!   executor applies them through this port.
//! - Atomicity: `stage_*` build a replacement in a hidden sibling of the target and never touch
//!   the target. `swap` then renames the replacement over it, which is atomic for one path, and
//!   keeps a backup so `restore` can undo it. Nothing here writes a target in place.
//! - File mode: `FileMode::Private` files are `0600` and the directories made for them `0700`.
//!   Every other file and directory is created with the process's default mode, and copies keep
//!   the permission bits of their source.
#![expect(
    clippy::disallowed_methods,
    clippy::disallowed_types,
    reason = "this module is the filesystem port; std's write APIs are banned everywhere else"
)]

use std::fs::{File, OpenOptions};
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result, ensure};

use crate::{
    file_op::{FileMode, FileOp},
    regular_files::regular_files,
};

/// What a new file holds and the permissions it gets.
pub(in crate::shell) struct NewFile<'a> {
    pub(in crate::shell) content: &'a [u8],
    pub(in crate::shell) mode: FileMode,
}

pub(in crate::shell) trait FileSystem {
    /// Creates the missing parent directories of `target` and returns them, outermost first.
    fn create_parents(&self, target: &Path, mode: FileMode) -> Result<Vec<PathBuf>>;
    /// Removes directories made by `create_parents`, innermost first, if they are still empty.
    fn remove_created(&self, directories: &[PathBuf]);
    /// Prepares a file replacement next to `target` without touching it.
    fn stage_file(&self, target: &Path, file: &NewFile<'_>) -> Result<PathBuf>;
    /// Prepares a copy of the file `source` next to `target` without touching it.
    fn stage_copy(&self, source: &Path, target: &Path) -> Result<PathBuf>;
    /// Prepares a copy of the directory `source` next to `target` without touching it.
    fn stage_directory(&self, source: &Path, target: &Path) -> Result<PathBuf>;
    /// Prepares a directory holding exactly `contents` next to `target` without touching it.
    fn stage_tree(&self, target: &Path, contents: &[FileOp]) -> Result<PathBuf>;
    /// Renames a staged path over `target`, returning the backup of what it replaced.
    fn swap(&self, staged: &Path, target: &Path) -> Result<Option<PathBuf>>;
    fn restore(&self, target: &Path, backup: Option<&Path>) -> Result<()>;
    /// Removes a staged path or a backup; a path that is already gone is not an error.
    fn discard(&self, path: &Path) -> Result<()>;
}

pub(in crate::shell) struct RealFileSystem;

impl FileSystem for RealFileSystem {
    fn create_parents(&self, target: &Path, mode: FileMode) -> Result<Vec<PathBuf>> {
        create_missing_parents(target, mode)
    }

    fn remove_created(&self, directories: &[PathBuf]) {
        remove_created_parents(directories);
    }

    fn stage_file(&self, target: &Path, file: &NewFile<'_>) -> Result<PathBuf> {
        let (path, handle) = create_temporary_file(target, file.mode)?;
        let written = fill(handle, file)
            .with_context(|| format!("failed to write temporary file for {}", target.display()));
        discard_unless_ok(written, &path)?;
        Ok(path)
    }

    fn stage_copy(&self, source: &Path, target: &Path) -> Result<PathBuf> {
        let staged = sibling_path(target, "tmp")?;
        discard_unless_ok(copy_file(source, &staged), &staged)?;
        Ok(staged)
    }

    fn stage_directory(&self, source: &Path, target: &Path) -> Result<PathBuf> {
        ensure!(
            source.is_dir(),
            "source directory does not exist: {}",
            source.display()
        );
        let staged = sibling_path(target, "tmp")?;
        discard_unless_ok(copy_tree(source, &staged), &staged)?;
        Ok(staged)
    }

    fn stage_tree(&self, target: &Path, contents: &[FileOp]) -> Result<PathBuf> {
        let staged = sibling_path(target, "tmp")?;
        discard_unless_ok(build_tree(target, &staged, contents), &staged)?;
        Ok(staged)
    }

    fn swap(&self, staged: &Path, target: &Path) -> Result<Option<PathBuf>> {
        swap_in(staged, target)
    }

    fn restore(&self, target: &Path, backup: Option<&Path>) -> Result<()> {
        restore(target, backup)
    }

    fn discard(&self, path: &Path) -> Result<()> {
        remove_path(path)
    }
}

/// Removes a half-built staged path when its construction failed, so a failure leaves nothing.
fn discard_unless_ok(result: Result<()>, staged: &Path) -> Result<()> {
    if result.is_err() {
        let _ = remove_path(staged);
    }
    result
}

/// A scratch directory under the system temporary directory, removed when dropped.
#[derive(Debug)]
pub(crate) struct TemporaryDirectory {
    path: PathBuf,
}

impl TemporaryDirectory {
    /// Creates a directory only the current user can enter.
    pub(crate) fn create() -> Result<Self> {
        create_staging_directory().map(|path| Self { path })
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn create_staging_directory() -> Result<PathBuf> {
    static NEXT_ID: AtomicU64 = AtomicU64::new(0);
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    builder.mode(0o700);
    loop {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("agent-harness-staging-{}-{id}", std::process::id()));
        let failure = format!("failed to create staging directory {}", path.display());
        match builder.create(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(error) => return Err(error).context(failure),
        }
    }
}

fn build_tree(root: &Path, staged: &Path, contents: &[FileOp]) -> Result<()> {
    create_directory(staged)?;
    for operation in contents {
        let target = staged.join(relative_to(root, operation.target())?);
        build_entry(operation, &target)?;
    }
    Ok(())
}

fn relative_to<'a>(root: &Path, path: &'a Path) -> Result<&'a Path> {
    path.strip_prefix(root).with_context(|| {
        format!(
            "{} is not inside the directory {} it is replacing",
            path.display(),
            root.display()
        )
    })
}

fn build_entry(operation: &FileOp, target: &Path) -> Result<()> {
    create_parent(target, FileMode::Standard)?;
    match operation {
        FileOp::WriteFile { content, mode, .. } => write_new_file(
            target,
            &NewFile {
                content,
                mode: *mode,
            },
        ),
        FileOp::CopyFile { source, .. } => copy_file(source, target),
        FileOp::CopyTree { source, .. } => {
            ensure!(
                source.is_dir(),
                "source directory does not exist: {}",
                source.display()
            );
            copy_tree(source, target)
        }
        FileOp::ReplaceTree {
            target: root,
            contents,
        } => build_tree(root, target, contents),
    }
}

/// Writes a file inside a directory that is still being staged, so it is never in use.
fn write_new_file(target: &Path, file: &NewFile<'_>) -> Result<()> {
    let handle = open_file(target, file.mode, false)
        .with_context(|| format!("failed to create {}", target.display()))?;
    fill(handle, file).with_context(|| format!("failed to write {}", target.display()))
}

fn fill(mut handle: File, file: &NewFile<'_>) -> std::io::Result<()> {
    handle.write_all(file.content)?;
    apply_exact_mode(&handle, file.mode)
}

#[cfg(unix)]
fn apply_exact_mode(handle: &File, mode: FileMode) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    match mode {
        FileMode::Bits(bits) => handle.set_permissions(std::fs::Permissions::from_mode(bits)),
        FileMode::Standard | FileMode::Private => Ok(()),
    }
}

#[cfg(not(unix))]
fn apply_exact_mode(_handle: &File, _mode: FileMode) -> std::io::Result<()> {
    Ok(())
}

fn create_directory(path: &Path) -> Result<()> {
    std::fs::create_dir(path)
        .with_context(|| format!("failed to create directory {}", path.display()))
}

fn copy_tree(source: &Path, staged: &Path) -> Result<()> {
    create_directory(staged)?;
    for file in regular_files(source)? {
        let relative = file
            .strip_prefix(source)
            .with_context(|| format!("failed to strip prefix {}", source.display()))?;
        let target = staged.join(relative);
        create_parent(&target, FileMode::Standard)?;
        copy_file(&file, &target)?;
    }
    Ok(())
}

fn copy_file(source: &Path, target: &Path) -> Result<()> {
    std::fs::copy(source, target).with_context(|| {
        format!(
            "failed to copy {} to {}",
            source.display(),
            target.display()
        )
    })?;
    Ok(())
}

/// Renames `staged` over `target`, returning the backup of what was there.
fn swap_in(staged: &Path, target: &Path) -> Result<Option<PathBuf>> {
    let backup = back_up(target)?;
    let renamed = std::fs::rename(staged, target)
        .with_context(|| format!("failed to replace {}", target.display()));
    let Err(error) = renamed else {
        return Ok(backup);
    };
    match restore(target, backup.as_deref()) {
        Ok(()) => Err(error),
        Err(restore_error) => Err(error.context(format!(
            "could not restore {} from {}: {restore_error:#}",
            target.display(),
            backup.as_deref().map_or_else(
                || "nothing (it did not exist)".to_owned(),
                |path| path.display().to_string()
            )
        ))),
    }
}

/// Backs up a regular file with a hard link so it stays in place until the rename replaces it.
fn back_up(target: &Path) -> Result<Option<PathBuf>> {
    let is_file = std::fs::symlink_metadata(target).is_ok_and(|metadata| metadata.is_file());
    if !is_file {
        return move_aside(target);
    }
    let backup = sibling_path(target, "bak")?;
    std::fs::hard_link(target, &backup)
        .or_else(|_| std::fs::copy(target, &backup).map(drop))
        .with_context(|| format!("failed to back up {}", target.display()))?;
    Ok(Some(backup))
}

/// Creates the missing parent directories of `target` and returns them, outermost first.
fn create_missing_parents(target: &Path, mode: FileMode) -> Result<Vec<PathBuf>> {
    let Some(parent) = target.parent() else {
        return Ok(Vec::new());
    };
    let mut missing: Vec<PathBuf> = parent
        .ancestors()
        .take_while(|ancestor| !ancestor.exists())
        .map(Path::to_path_buf)
        .collect();
    missing.reverse();
    if let Err(error) = create_parent(target, mode) {
        remove_created_parents(&missing);
        return Err(error);
    }
    Ok(missing)
}

/// Removes directories `create_missing_parents` made, innermost first, if they are still empty.
fn remove_created_parents(created: &[PathBuf]) {
    for directory in created.iter().rev() {
        let _ = std::fs::remove_dir(directory);
    }
}

fn create_parent(target: &Path, mode: FileMode) -> Result<()> {
    let Some(parent) = target.parent() else {
        return Ok(());
    };
    directory_builder(mode)
        .create(parent)
        .with_context(|| format!("failed to create directory {}", parent.display()))
}

fn directory_builder(mode: FileMode) -> std::fs::DirBuilder {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    if mode == FileMode::Private {
        builder.mode(0o700);
    }
    #[cfg(not(unix))]
    let _ = mode;
    builder
}

/// Renames an existing `target` to a sibling backup, or returns `None` when nothing exists.
fn move_aside(target: &Path) -> Result<Option<PathBuf>> {
    if std::fs::symlink_metadata(target).is_err() {
        return Ok(None);
    }
    let backup = sibling_path(target, "bak")?;
    std::fs::rename(target, &backup)
        .with_context(|| format!("failed to move {} aside", target.display()))?;
    Ok(Some(backup))
}

/// Puts `backup` back at `target`, discarding whatever was published there meanwhile.
fn restore(target: &Path, backup: Option<&Path>) -> Result<()> {
    let Some(backup) = backup else {
        return remove_path(target);
    };
    let replaceable_by_rename =
        |path: &Path| std::fs::symlink_metadata(path).is_ok_and(|metadata| !metadata.is_dir());
    if !(replaceable_by_rename(target) && replaceable_by_rename(backup)) {
        remove_path(target)?;
    }
    std::fs::rename(backup, target)
        .with_context(|| format!("failed to restore {}", target.display()))
}

fn remove_path(path: &Path) -> Result<()> {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return Ok(());
    };
    let removal = if metadata.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    };
    removal.with_context(|| format!("failed to remove {}", path.display()))
}

fn sibling_path(target: &Path, label: &str) -> Result<PathBuf> {
    static NEXT_ID: AtomicU64 = AtomicU64::new(0);
    let name = target.file_name().context("output path must name a file")?;
    let candidate = || {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let mut sibling_name = std::ffi::OsString::from(".");
        sibling_name.push(name);
        sibling_name.push(format!(".{}.{id}.{label}", std::process::id()));
        target.with_file_name(sibling_name)
    };
    std::iter::repeat_with(candidate)
        .find(|path| std::fs::symlink_metadata(path).is_err())
        .context("no free sibling path")
}

fn create_temporary_file(target: &Path, mode: FileMode) -> Result<(PathBuf, File)> {
    static NEXT_ID: AtomicU64 = AtomicU64::new(0);
    let name = target.file_name().context("output path must name a file")?;
    let failure = format!("failed to create temporary file for {}", target.display());
    loop {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let mut temporary_name = name.to_os_string();
        temporary_name.push(format!(".{}.{id}.tmp", std::process::id()));
        let path = target.with_file_name(temporary_name);
        match open_file(&path, mode, true) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(error) => return Err(error).context(failure),
        }
    }
}

/// With `must_be_new` the open fails rather than reuse a file, which keeps temporary names safe.
fn open_file(path: &Path, mode: FileMode, must_be_new: bool) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true);
    if must_be_new {
        options.create_new(true);
    } else {
        options.create(true).truncate(true);
    }
    #[cfg(unix)]
    if mode == FileMode::Private {
        options.mode(0o600);
    }
    #[cfg(not(unix))]
    let _ = mode;
    options.open(path)
}

#[cfg(all(test, unix))]
mod tests;
