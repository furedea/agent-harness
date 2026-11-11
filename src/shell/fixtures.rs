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

#[cfg(unix)]
pub(crate) fn symlink(original: impl AsRef<Path>, link: impl AsRef<Path>) -> Result<()> {
    std::os::unix::fs::symlink(original, link)
}

/// A directory read into a snapshot, for tests that build their inputs on disk.
pub(crate) struct LoadedTree {
    root: std::path::PathBuf,
    files: crate::snapshot::Snapshot,
}

impl LoadedTree {
    pub(crate) fn tree(&self) -> crate::snapshot::Tree<'_> {
        crate::snapshot::Tree::new(&self.root, &self.files)
    }

    pub(crate) fn files(&self) -> &crate::snapshot::Snapshot {
        &self.files
    }
}

/// Reads `root` and every directory in `extra` into one snapshot.
pub(crate) fn load_trees(root: &Path, extra: &[&Path]) -> anyhow::Result<LoadedTree> {
    let request = crate::snapshot::SnapshotRequest {
        trees: std::iter::once(root)
            .chain(extra.iter().copied())
            .map(Path::to_path_buf)
            .collect(),
        ..crate::snapshot::SnapshotRequest::default()
    };
    Ok(LoadedTree {
        root: root.to_path_buf(),
        files: super::load_snapshot(&request)?,
    })
}

pub(crate) fn load_tree(root: &Path) -> anyhow::Result<LoadedTree> {
    load_trees(root, &[])
}

/// Reads the text of each of `paths` into a snapshot, for sync tests.
pub(crate) fn load_documents(paths: &[&Path]) -> anyhow::Result<crate::snapshot::Snapshot> {
    let request = crate::snapshot::SnapshotRequest {
        documents: paths.iter().map(|path| path.to_path_buf()).collect(),
        ..crate::snapshot::SnapshotRequest::default()
    };
    super::load_snapshot(&request)
}
