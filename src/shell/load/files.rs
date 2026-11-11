//! The disk primitives of the loader: text, directory listings, and the walk of a copied tree.
//!
//! A tree is walked following a symlinked directory one hop (a profile may share a `hooks`
//! directory), never a symlink met inside that one, so a link cycle cannot loop and a copy never
//! reaches outside its directory.
#![expect(
    clippy::disallowed_methods,
    reason = "this module is the read side of the shell; computation only sees the Source"
)]

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::Value;
use toml_edit::DocumentMut;

use crate::generation::external_hooks;

/// The UTF-8 text of `path`.
pub(super) fn read_text(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))
}

/// The UTF-8 text of `path`, or `None` when it does not exist or is a symlink that cannot be
/// followed.
pub(super) fn read_text_if_present(path: &Path) -> Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) if is_symlink(path) && !path.exists() => Ok(None),
        Err(error) => Err(error).with_context(|| format!("failed to read {}", path.display())),
    }
}

pub(super) fn read_json(path: &Path) -> Result<Value> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read JSON file {}", path.display()))?;
    serde_json::from_str(&text)
        .with_context(|| format!("failed to parse JSON file {}", path.display()))
}

pub(super) fn read_toml(path: &Path) -> Result<DocumentMut> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read TOML file {}", path.display()))?;
    text.parse::<DocumentMut>()
        .with_context(|| format!("failed to parse TOML file {}", path.display()))
}

/// The regular shell scripts directly in `dir`, sorted; a symlink is not one.
pub(crate) fn shell_scripts(dir: &Path) -> Result<Vec<PathBuf>> {
    Ok(children(dir)?
        .into_iter()
        .filter(|child| child.is_regular_file() && external_hooks::is_codex_script(&child.path))
        .map(|child| child.path)
        .collect())
}

/// A child of a directory, with what it is without following a symlink and what it points to.
#[derive(Debug)]
pub(super) struct Child {
    pub(super) name: OsString,
    pub(super) path: PathBuf,
    is_symlink: bool,
    is_directory: bool,
    is_file: bool,
}

impl Child {
    fn of(path: PathBuf, is_symlink: bool) -> Option<Self> {
        let followed = std::fs::metadata(&path);
        Some(Self {
            name: path.file_name()?.to_owned(),
            is_directory: followed.as_ref().is_ok_and(std::fs::Metadata::is_dir),
            is_file: followed.as_ref().is_ok_and(std::fs::Metadata::is_file),
            is_symlink,
            path,
        })
    }

    /// A regular file: not a symlink, so a copy never reaches outside its own directory.
    pub(super) fn is_regular_file(&self) -> bool {
        self.is_file && !self.is_symlink
    }

    /// A real directory: a symlink to one is not followed.
    pub(super) fn is_real_directory(&self) -> bool {
        self.is_directory && !self.is_symlink
    }
}

/// The direct children of `dir`, sorted by path; a missing `dir` has none.
pub(super) fn children(dir: &Path) -> Result<Vec<Child>> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut children: Vec<Child> = list_directory(dir)?
        .into_iter()
        .filter_map(|(path, is_symlink)| Child::of(path, is_symlink))
        .collect();
    children.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(children)
}

/// The regular files under `dir`, sorted; `dir` itself may be a symlink, anything behind a symlink
/// below it is skipped.
pub(crate) fn regular_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_regular_files(dir, &mut files)?;
    files.sort();
    Ok(files)
}

pub(super) fn collect_regular_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    list_directory(dir)?
        .into_iter()
        .filter(|(_, is_symlink)| !is_symlink)
        .try_for_each(|(path, _)| collect_entry(&path, files))
}

pub(super) fn collect_entry(path: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    let metadata =
        std::fs::metadata(path).with_context(|| format!("failed to inspect {}", path.display()))?;
    if metadata.is_dir() {
        return collect_regular_files(path, files);
    }
    if metadata.is_file() {
        files.push(path.to_path_buf());
    }
    Ok(())
}

/// The entries of `dir` with whether each is a symlink, which `read_dir` reports without
/// following.
pub(super) fn list_directory(dir: &Path) -> Result<Vec<(PathBuf, bool)>> {
    std::fs::read_dir(dir)
        .with_context(|| format!("failed to read directory {}", dir.display()))?
        .map(|entry| {
            let entry =
                entry.with_context(|| format!("failed to read entry in {}", dir.display()))?;
            let file_type = entry
                .file_type()
                .with_context(|| format!("failed to inspect {}", entry.path().display()))?;
            Ok((entry.path(), file_type.is_symlink()))
        })
        .collect()
}

pub(super) fn is_symlink(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_symlink())
}

pub(super) fn is_regular_file(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_file())
}
