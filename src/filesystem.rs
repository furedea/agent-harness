use std::path::Path;

use anyhow::{Context, Result};

use crate::fs_ops;
pub(crate) use crate::fs_ops::TemporaryDirectory;

pub(crate) trait FileSystem {
    fn copy_file(&self, source: &Path, target: &Path) -> Result<()>;
    fn replace_directory(&self, source: &Path, target: &Path) -> Result<()>;
    fn write_file(&self, target: &Path, content: &[u8]) -> Result<()>;
    fn read_file(&self, source: &Path) -> Result<Vec<u8>>;
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
}
