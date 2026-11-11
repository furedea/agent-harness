//! The config that is already installed, parsed once at the boundary.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::codec::{ConfigDocument, Role};
use crate::{file_op::FileOp, provider::ownership::ConfigSync, snapshot::Snapshot};

/// What a sync leaves to write: the config and, when a claim tracks keys, its sidecar.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Synced {
    pub(crate) config: String,
    pub(crate) sidecar: Option<String>,
}

impl Synced {
    /// The writes that put this result at `target`, the sidecar next to it.
    ///
    /// # Errors
    ///
    /// Returns an error when the sidecar was produced for a config that declares none.
    pub(crate) fn into_ops(self, sync: &ConfigSync, target: &Path) -> Result<Vec<FileOp>> {
        let sidecar = self
            .sidecar
            .map(|content| sidecar_op(sync, target, content))
            .transpose()?;
        Ok(std::iter::once(FileOp::write(target, self.config))
            .chain(sidecar)
            .collect())
    }
}

fn sidecar_op(sync: &ConfigSync, target: &Path, content: String) -> Result<FileOp> {
    let file = sync
        .sidecar_file()
        .context("a sidecar needs the config to declare one")?;
    Ok(FileOp::write(sidecar_path(target, file), content))
}

/// The keys each tracking claim wrote last, by the claim's path joined with `_`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Tracked(BTreeMap<String, BTreeSet<String>>);

impl Tracked {
    /// A sidecar that is missing or unreadable is `None`: it must not block the sync, it only
    /// forgoes removals.
    fn parse(text: Option<&str>) -> Option<Self> {
        let text = text?;
        serde_json::from_str(text).ok().map(Self)
    }

    /// The keys written for the claim named `name`, or `formerly_managed` while there is no
    /// sidecar to ask.
    pub(crate) fn previous(
        known: Option<&Self>,
        name: &str,
        formerly_managed: &[&str],
    ) -> BTreeSet<String> {
        match known {
            Some(tracked) => tracked.0.get(name).cloned().unwrap_or_default(),
            None => formerly_managed.iter().map(ToString::to_string).collect(),
        }
    }

    pub(crate) fn record(&mut self, name: String, keys: BTreeSet<String>) {
        self.0.insert(name, keys);
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(crate) fn render(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(&self.0)? + "\n")
    }
}

/// The text of an installed config and of its sidecar, when they exist.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct InstalledText<'a> {
    pub(crate) config: Option<&'a str>,
    pub(crate) sidecar: Option<&'a str>,
}

/// The config of a provider as the user has it, with the state the harness keeps beside it.
#[derive(Debug)]
pub(crate) struct Installed {
    document: Option<ConfigDocument>,
    /// `None` when there is no readable sidecar.
    tracked: Option<Tracked>,
}

impl Installed {
    /// # Errors
    ///
    /// Returns an error naming `path` when the config exists and cannot be parsed.
    pub(crate) fn parse(sync: &ConfigSync, path: &Path, text: InstalledText<'_>) -> Result<Self> {
        let document = text
            .config
            .map(|text| ConfigDocument::parse(sync.format, text, Role::Installed))
            .transpose()
            .with_context(|| format!("failed to parse {}", path.display()))?;
        Ok(Self {
            document,
            tracked: Tracked::parse(text.sidecar),
        })
    }

    /// Reads the config at `path` and its sidecar from `files`.
    ///
    /// # Errors
    ///
    /// Returns an error when the config exists but cannot be read or parsed.
    pub(crate) fn from_snapshot(sync: &ConfigSync, files: &Snapshot, path: &Path) -> Result<Self> {
        let text = files.read_text_if_present(path)?;
        let sidecar = sync
            .sidecar_file()
            .map(|file| sidecar_path(path, file))
            .and_then(|path| files.read_text_if_present(&path).ok().flatten());
        Self::parse(
            sync,
            path,
            InstalledText {
                config: text,
                sidecar,
            },
        )
    }

    pub(crate) fn exists(&self) -> bool {
        self.document.is_some()
    }

    pub(crate) fn into_parts(self) -> (Option<ConfigDocument>, Option<Tracked>) {
        (self.document, self.tracked)
    }
}

/// The sidecar `file` that sits next to the config at `target`.
pub(crate) fn sidecar_path(target: &Path, file: &str) -> PathBuf {
    target.with_file_name(file)
}

/// The paths a sync reads besides the rendered config: the installed config and its sidecar.
pub(crate) fn installed_files(sync: &ConfigSync, target: &Path) -> Vec<PathBuf> {
    std::iter::once(target.to_path_buf())
        .chain(sync.sidecar_file().map(|file| sidecar_path(target, file)))
        .collect()
}

/// The rendered config at `path`, or an empty one when the file does not exist.
///
/// # Errors
///
/// Returns an error naming `path` when it exists and cannot be read or parsed.
pub(crate) fn read_optional_generated(
    sync: &ConfigSync,
    files: &Snapshot,
    path: &Path,
) -> Result<ConfigDocument> {
    match files.read_text_if_present(path)? {
        Some(_) => read_generated(sync, files, path),
        None => Ok(ConfigDocument::blank(sync.format)),
    }
}

/// The rendered config at `path`.
///
/// # Errors
///
/// Returns an error naming `path` when it cannot be read or parsed.
pub(crate) fn read_generated(
    sync: &ConfigSync,
    files: &Snapshot,
    path: &Path,
) -> Result<ConfigDocument> {
    let text = files
        .read_text(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    ConfigDocument::parse(sync.format, text, Role::Generated)
        .with_context(|| format!("failed to parse {}", path.display()))
}
