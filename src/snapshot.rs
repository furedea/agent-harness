#![forbid(clippy::disallowed_methods, clippy::disallowed_types)]
//! What computation knows about the disk: a value read once, at the boundary.
//!
//! A computation that read files while it ran would depend on the disk at that instant. The shell
//! loads the files a computation needs into a [`Snapshot`] first (see `shell::load_snapshot`), and
//! the computation answers every question about the disk from that value alone, so one snapshot
//! always yields one result.
//!
//! A snapshot is keyed by absolute path and mixes any number of roots: the harness source, the
//! external hook bundles, the installed configs a sync merges into. Directories are recorded with
//! their entries, and text is recorded only for the files the loader was told to read; whole
//! directories that are merely copied need no contents, only the listing that decisions rest on.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::ops::Bound;
use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};

/// What a path resolved to when the snapshot was taken, following symlinks like `Path::is_file`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EntryKind {
    File,
    Directory,
    /// A socket or anything else that exists but is neither file nor directory.
    Other,
    /// A symlink that cannot be followed (dangling, looping, or into a place it may not enter),
    /// which `Path::exists` reports as absent.
    Unreachable,
}

/// One path in a snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Entry {
    kind: EntryKind,
    is_symlink: bool,
    text: Option<String>,
}

impl Entry {
    pub(crate) fn new(kind: EntryKind, is_symlink: bool) -> Self {
        Self {
            kind,
            is_symlink,
            text: None,
        }
    }

    pub(crate) fn is_file_kind(&self) -> bool {
        self.kind == EntryKind::File
    }

    fn text(&self, path: &Path) -> Result<&str> {
        self.text.as_deref().ok_or_else(|| unreadable(path))
    }

    /// Records the UTF-8 content of a file entry.
    #[must_use]
    pub(crate) fn with_text(mut self, text: String) -> Self {
        self.text = Some(text);
        self
    }
}

/// A child of a directory, as listed by [`Snapshot::children`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Child {
    pub(crate) name: OsString,
    pub(crate) path: PathBuf,
    pub(crate) kind: EntryKind,
    pub(crate) is_symlink: bool,
}

impl Child {
    fn of(path: &Path, entry: &Entry) -> Option<Self> {
        Some(Self {
            name: path.file_name()?.to_owned(),
            path: path.to_path_buf(),
            kind: entry.kind,
            is_symlink: entry.is_symlink,
        })
    }

    /// A regular file: not a symlink, so a copy never reaches outside its own directory.
    pub(crate) fn is_regular_file(&self) -> bool {
        self.kind == EntryKind::File && !self.is_symlink
    }

    /// A real directory: a symlink to one is not followed.
    pub(crate) fn is_real_directory(&self) -> bool {
        self.kind == EntryKind::Directory && !self.is_symlink
    }
}

#[derive(Debug, Default)]
pub(crate) struct Snapshot {
    entries: BTreeMap<PathBuf, Entry>,
}

impl Snapshot {
    pub(crate) fn insert(&mut self, path: PathBuf, entry: Entry) {
        self.entries.insert(path, entry);
    }

    /// Whether `path` is a file, or a symlink to one.
    pub(crate) fn is_file(&self, path: &Path) -> bool {
        self.kind(path) == Some(EntryKind::File)
    }

    /// Whether `path` is a directory, or a symlink to one.
    pub(crate) fn is_dir(&self, path: &Path) -> bool {
        self.kind(path) == Some(EntryKind::Directory)
    }

    /// Whether `path` is a regular file itself, not a symlink.
    pub(crate) fn is_regular_file(&self, path: &Path) -> bool {
        self.entries
            .get(path)
            .is_some_and(|entry| entry.kind == EntryKind::File && !entry.is_symlink)
    }

    /// Whether `path` exists, following symlinks like `Path::exists`.
    pub(crate) fn exists(&self, path: &Path) -> bool {
        self.kind(path)
            .is_some_and(|kind| kind != EntryKind::Unreachable)
    }

    /// The text of `path`.
    ///
    /// # Errors
    ///
    /// Returns an error when the path is absent or was not loaded as UTF-8 text.
    pub(crate) fn read_text(&self, path: &Path) -> Result<&str> {
        self.read_text_if_present(path)?
            .ok_or_else(|| anyhow!("{} does not exist", path.display()))
    }

    /// The text of `path`, or `None` when the path is absent.
    ///
    /// # Errors
    ///
    /// Returns an error when the path exists but was not loaded as UTF-8 text.
    pub(crate) fn read_text_if_present(&self, path: &Path) -> Result<Option<&str>> {
        match self.entries.get(path) {
            Some(entry) if entry.kind != EntryKind::Unreachable => entry.text(path).map(Some),
            _ => Ok(None),
        }
    }

    /// The direct children of `dir`, sorted by path; a missing or empty `dir` has none.
    pub(crate) fn children(&self, dir: &Path) -> Vec<Child> {
        self.entries
            .range::<Path, _>(from(dir))
            .take_while(|(path, _)| path.starts_with(dir))
            .filter(|(path, _)| path.parent() == Some(dir))
            .filter_map(|(path, entry)| Child::of(path, entry))
            .collect()
    }

    /// The regular files under `dir`, sorted; a missing `dir` has none.
    ///
    /// Anything behind a symlink below `dir` is skipped, so a copy of `dir` never reaches outside
    /// it; `dir` itself may be a symlink.
    pub(crate) fn regular_files(&self, dir: &Path) -> Vec<PathBuf> {
        self.entries
            .range::<Path, _>(from(dir))
            .take_while(|(path, _)| path.starts_with(dir))
            .filter(|(path, entry)| path.as_path() != dir && entry.kind == EntryKind::File)
            .filter(|(path, _)| !self.is_behind_symlink(dir, path))
            .map(|(path, _)| path.clone())
            .collect()
    }

    /// Whether `path` or a directory between it and `dir` is a symlink.
    fn is_behind_symlink(&self, dir: &Path, path: &Path) -> bool {
        path.ancestors()
            .take_while(|ancestor| *ancestor != dir)
            .any(|ancestor| self.is_symlink(ancestor))
    }

    fn is_symlink(&self, path: &Path) -> bool {
        self.entries.get(path).is_some_and(|entry| entry.is_symlink)
    }

    fn kind(&self, path: &Path) -> Option<EntryKind> {
        self.entries.get(path).map(|entry| entry.kind)
    }
}

fn unreadable(path: &Path) -> anyhow::Error {
    anyhow!(
        "{} is not a file that could be read as UTF-8 text",
        path.display()
    )
}

fn from(dir: &Path) -> (Bound<&Path>, Bound<&Path>) {
    (Bound::Included(dir), Bound::Unbounded)
}

/// What a computation needs read from the disk, described as data for the loader.
#[derive(Debug, Default)]
pub(crate) struct SnapshotRequest {
    /// Directories recorded recursively; a missing one records nothing.
    pub(crate) trees: Vec<PathBuf>,
    /// Directories whose direct entries are recorded, without going into them.
    pub(crate) listings: Vec<PathBuf>,
    /// Files whose UTF-8 text is recorded, whatever their extension; a missing one records
    /// nothing, and one that is not UTF-8 is recorded without text.
    pub(crate) documents: Vec<PathBuf>,
    /// Paths whose kind alone is recorded, so computation can ask whether they exist.
    pub(crate) probes: Vec<PathBuf>,
}

/// A root directory together with the snapshot that describes it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Tree<'a> {
    root: &'a Path,
    files: &'a Snapshot,
}

impl<'a> Tree<'a> {
    pub(crate) fn new(root: &'a Path, files: &'a Snapshot) -> Self {
        Self { root, files }
    }

    pub(crate) fn root(self) -> &'a Path {
        self.root
    }

    pub(crate) fn files(self) -> &'a Snapshot {
        self.files
    }
}

#[cfg(test)]
mod tests;
