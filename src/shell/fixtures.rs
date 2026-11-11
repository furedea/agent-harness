//! Test-only filesystem helpers for computation modules, which may not call std's write APIs.
#![allow(
    clippy::disallowed_methods,
    reason = "test fixtures build their own temporary trees"
)]

use std::io::Result;
use std::path::Path;

pub(crate) fn write(path: impl AsRef<Path>, content: impl AsRef<[u8]>) -> Result<()> {
    std::fs::write(path, content)
}

pub(crate) fn create_dir_all(path: impl AsRef<Path>) -> Result<()> {
    std::fs::create_dir_all(path)
}

pub(crate) fn create_dir(path: impl AsRef<Path>) -> Result<()> {
    std::fs::create_dir(path)
}

pub(crate) fn remove_dir_all(path: impl AsRef<Path>) -> Result<()> {
    std::fs::remove_dir_all(path)
}

pub(crate) fn remove_file(path: impl AsRef<Path>) -> Result<()> {
    std::fs::remove_file(path)
}

pub(crate) fn rename(from: impl AsRef<Path>, to: impl AsRef<Path>) -> Result<()> {
    std::fs::rename(from, to)
}
