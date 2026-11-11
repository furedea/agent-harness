use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::fs_ops;
pub(crate) use crate::fs_ops::TemporaryDirectory;

pub(crate) trait FileSystem {
    fn copy_file(&self, source: &Path, target: &Path) -> Result<()>;
    fn replace_directory(&self, source: &Path, target: &Path) -> Result<()>;
    fn write_file(&self, target: &Path, content: &[u8]) -> Result<()>;
    fn read_file(&self, source: &Path) -> Result<Vec<u8>>;
    /// Moves an existing path to a backup location so that a failed publication can undo it.
    fn move_aside(&self, target: &Path) -> Result<Option<PathBuf>>;
    fn restore(&self, target: &Path, backup: Option<&Path>) -> Result<()>;
    fn discard(&self, backup: &Path) -> Result<()>;
}

pub(crate) struct RealFileSystem;

impl FileSystem for RealFileSystem {
    fn copy_file(&self, source: &Path, target: &Path) -> Result<()> {
        fs_ops::copy_file(source, target)
    }

    fn replace_directory(&self, source: &Path, target: &Path) -> Result<()> {
        fs_ops::copy_dir(source, target)
    }

    fn write_file(&self, target: &Path, content: &[u8]) -> Result<()> {
        fs_ops::write_file_atomically(target, content)
    }

    fn read_file(&self, source: &Path) -> Result<Vec<u8>> {
        std::fs::read(source)
            .with_context(|| format!("failed to read rendered file {}", source.display()))
    }

    fn move_aside(&self, target: &Path) -> Result<Option<PathBuf>> {
        fs_ops::move_aside(target)
    }

    fn restore(&self, target: &Path, backup: Option<&Path>) -> Result<()> {
        fs_ops::restore(target, backup)
    }

    fn discard(&self, backup: &Path) -> Result<()> {
        fs_ops::discard(backup)
    }
}
