use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::fs_ops;
pub(crate) use crate::fs_ops::TemporaryDirectory;

pub(crate) trait FileSystem {
    fn copy_file(&self, source: &Path, target: &Path) -> Result<()>;
    fn replace_directory(&self, source: &Path, target: &Path) -> Result<()>;
    fn read_file(&self, source: &Path) -> Result<Vec<u8>>;
    /// Prepares a replacement next to `target` without touching it.
    fn stage_directory(&self, source: &Path, target: &Path) -> Result<PathBuf>;
    fn stage_file(&self, target: &Path, content: &[u8]) -> Result<PathBuf>;
    /// Renames a staged path over `target`, returning the backup of what it replaced.
    fn swap(&self, staged: &Path, target: &Path) -> Result<Option<PathBuf>>;
    fn restore(&self, target: &Path, backup: Option<&Path>) -> Result<()>;
    /// Removes a staged path or a backup; a path that is already gone is not an error.
    fn discard(&self, path: &Path) -> Result<()>;
}

pub(crate) struct RealFileSystem;

impl FileSystem for RealFileSystem {
    fn copy_file(&self, source: &Path, target: &Path) -> Result<()> {
        fs_ops::copy_file(source, target)
    }

    fn replace_directory(&self, source: &Path, target: &Path) -> Result<()> {
        fs_ops::copy_dir(source, target)
    }

    fn read_file(&self, source: &Path) -> Result<Vec<u8>> {
        std::fs::read(source)
            .with_context(|| format!("failed to read rendered file {}", source.display()))
    }

    fn stage_directory(&self, source: &Path, target: &Path) -> Result<PathBuf> {
        fs_ops::stage_directory(source, target)
    }

    fn stage_file(&self, target: &Path, content: &[u8]) -> Result<PathBuf> {
        fs_ops::stage_file(target, content)
    }

    fn swap(&self, staged: &Path, target: &Path) -> Result<Option<PathBuf>> {
        fs_ops::swap_in(staged, target)
    }

    fn restore(&self, target: &Path, backup: Option<&Path>) -> Result<()> {
        fs_ops::restore(target, backup)
    }

    fn discard(&self, path: &Path) -> Result<()> {
        fs_ops::discard(path)
    }
}
