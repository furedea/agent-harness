//! The loader: reads the disk once and hands computation a [`Snapshot`].
//!
//! Computation never reads files, so everything it decides on is gathered here first. A tree is
//! walked following a symlinked directory one hop (a profile may share a `hooks` directory), never
//! a symlink met inside that one, so a link cycle cannot loop. Each symlink is recorded as the kind
//! it points to, as `Path::is_file` would answer, and the snapshot's `regular_files` skips what
//! lies behind one, as the directory walk it replaces did.
#![expect(
    clippy::disallowed_methods,
    reason = "this module is the read side of the shell; computation only sees the snapshot"
)]

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::{
    generation::{
        codex_managed_state::managed_state_path, external_hooks::ExternalHookBundle,
        skills::ExternalSkill,
    },
    provider::spec::ConfigSync,
    snapshot::{Entry, EntryKind, Snapshot, SnapshotRequest},
};

/// Extensions whose content a walked tree records as text: the documents computation parses.
const TEXT_EXTENSIONS: &[&str] = &["json", "toml", "yaml", "yml", "md"];

/// Extra inputs a generation run takes besides the source tree.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ExternalInputs<'a> {
    pub(crate) hook_bundles: &'a [ExternalHookBundle],
    pub(crate) skills: &'a [ExternalSkill],
}

/// The source tree, every external hook bundle, and the skill file of every external skill.
pub(crate) fn source_request(source: &Path, external: ExternalInputs<'_>) -> SnapshotRequest {
    let bundles = external.hook_bundles.iter().map(ExternalHookBundle::source);
    SnapshotRequest {
        trees: std::iter::once(source)
            .chain(bundles)
            .map(Path::to_path_buf)
            .collect(),
        probes: external
            .skills
            .iter()
            .map(ExternalSkill::skill_file)
            .collect(),
        ..SnapshotRequest::default()
    }
}

/// The files a sync of `sync` reads from its user-owned `target`.
pub(crate) fn sync_documents(sync: ConfigSync, target: &Path) -> Vec<PathBuf> {
    let mut documents = vec![target.to_path_buf()];
    if sync == ConfigSync::MergeToml {
        documents.push(managed_state_path(target));
    }
    documents
}

/// Reads what `request` names into a snapshot.
///
/// # Errors
///
/// Returns an error when a listed directory or a text file cannot be read.
pub(crate) fn load_snapshot(request: &SnapshotRequest) -> Result<Snapshot> {
    let mut snapshot = Snapshot::default();
    // Each pass records at least what the one before did, so a path named twice keeps its text.
    for probe in &request.probes {
        record_single(&mut snapshot, probe, false)?;
    }
    for listing in &request.listings {
        record_listing(&mut snapshot, listing)?;
    }
    for tree in &request.trees {
        record_tree(&mut snapshot, tree)?;
    }
    for document in &request.documents {
        record_single(&mut snapshot, document, true)?;
    }
    Ok(snapshot)
}

fn record_tree(snapshot: &mut Snapshot, root: &Path) -> Result<()> {
    record_path(snapshot, root, Visit::default())
}

fn record_single(snapshot: &mut Snapshot, path: &Path, wants_text: bool) -> Result<()> {
    let is_symlink = std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_symlink());
    let Some(kind) = resolve_kind(path, is_symlink)? else {
        return Ok(());
    };
    let entry = entry_of(path, Entry::new(kind, is_symlink), wants_text)?;
    snapshot.insert(path.to_path_buf(), entry);
    Ok(())
}

/// How a path was reached, which decides whether the walk goes into it.
#[derive(Clone, Copy, Debug, Default)]
struct Visit {
    is_symlink: bool,
    /// Whether the walk already crossed a symlink to get here.
    inside_link: bool,
}

impl Visit {
    fn descends_into_directory(self) -> bool {
        !(self.is_symlink && self.inside_link)
    }

    fn child(self, is_symlink: bool) -> Self {
        Self {
            is_symlink,
            inside_link: self.inside_link || self.is_symlink,
        }
    }
}

/// Records `path`, and what is under it when it is a directory the walk goes into.
fn record_path(snapshot: &mut Snapshot, path: &Path, visit: Visit) -> Result<()> {
    let Some(kind) = resolve_kind(path, visit.is_symlink)? else {
        return Ok(());
    };
    let wants_text = has_text_extension(path);
    let entry = Entry::new(kind, visit.is_symlink);
    snapshot.insert(path.to_path_buf(), entry_of(path, entry, wants_text)?);
    if kind == EntryKind::Directory && visit.descends_into_directory() {
        return record_children(snapshot, path, visit);
    }
    Ok(())
}

/// Records `dir` and its direct entries, going into none of them.
fn record_listing(snapshot: &mut Snapshot, dir: &Path) -> Result<()> {
    record_single(snapshot, dir, false)?;
    if !snapshot.is_dir(dir) {
        return Ok(());
    }
    for (path, is_symlink) in list_directory(dir)? {
        record_shallow(snapshot, path, is_symlink)?;
    }
    Ok(())
}

fn record_shallow(snapshot: &mut Snapshot, path: PathBuf, is_symlink: bool) -> Result<()> {
    let Some(kind) = resolve_kind(&path, is_symlink)? else {
        return Ok(());
    };
    snapshot.insert(path, Entry::new(kind, is_symlink));
    Ok(())
}

/// The real path of `path`, which must exist.
pub(crate) fn real_path(path: &Path) -> Result<PathBuf> {
    std::fs::canonicalize(path).with_context(|| format!("failed to resolve {}", path.display()))
}

fn record_children(snapshot: &mut Snapshot, dir: &Path, visit: Visit) -> Result<()> {
    for (path, is_symlink) in list_directory(dir)? {
        record_path(snapshot, &path, visit.child(is_symlink))?;
    }
    Ok(())
}

/// The entries of `dir` with whether each is a symlink, which `read_dir` reports without
/// following.
fn list_directory(dir: &Path) -> Result<Vec<(PathBuf, bool)>> {
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

/// What `path` resolves to, or `None` when it does not exist.
///
/// A symlink that cannot be followed is `Other`; other failures are errors.
fn resolve_kind(path: &Path, is_symlink: bool) -> Result<Option<EntryKind>> {
    match std::fs::metadata(path) {
        Ok(metadata) => Ok(Some(kind_of(&metadata))),
        // A link that cannot be followed (broken, looping, or into a place it may not enter) is
        // `Other`, as `Path::is_file` would answer for it.
        Err(_) if is_symlink => Ok(Some(EntryKind::Other)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("failed to inspect {}", path.display())),
    }
}

fn kind_of(metadata: &std::fs::Metadata) -> EntryKind {
    if metadata.is_dir() {
        EntryKind::Directory
    } else if metadata.is_file() {
        EntryKind::File
    } else {
        EntryKind::Other
    }
}

/// The entry for `path`, with its text when wanted and the file is UTF-8.
fn entry_of(path: &Path, entry: Entry, wants_text: bool) -> Result<Entry> {
    if !wants_text || !entry.is_file_kind() {
        return Ok(entry);
    }
    let bytes =
        std::fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    Ok(match String::from_utf8(bytes) {
        Ok(text) => entry.with_text(text),
        Err(_) => entry,
    })
}

fn has_text_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| TEXT_EXTENSIONS.contains(&extension))
}

#[cfg(test)]
mod tests;
