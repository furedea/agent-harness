#![forbid(clippy::disallowed_methods, clippy::disallowed_types)]
//! What computation knows about the source: documents parsed once, at the boundary.
//!
//! The clerk at the window reads the documents and fills out a fixed form; computation reads only
//! the form. `shell::load_source` reads the source tree, the external hook bundles, and the
//! external skills, parses and validates every document (an error names the file it came from),
//! and returns a [`Source`]. Computation takes `&Source` and returns `FileOp` values; it neither
//! looks files up by path nor parses text.
//!
//! A directory that is only copied is a location and a list of files ([`SourceDir`]); its content
//! is never loaded, because the executor copies it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::Value;
use toml_edit::DocumentMut;

use crate::{
    config_sync::codec::ConfigDocument,
    file_op::FileOp,
    generation::{
        command_permissions::{CommandPermissions, RegexPolicy},
        hooks::HookConfig,
        secret_path_policy::SecretPathPolicy,
        skills::{ExternalSkill, SourceSkills},
    },
    manifest::Manifest,
};

/// A source tree, parsed and validated.
#[derive(Debug)]
pub(crate) struct Source {
    pub(crate) root: PathBuf,
    pub(crate) manifest: Manifest,
    pub(crate) hooks: HookConfig,
    pub(crate) command_permissions: CommandPermissions,
    pub(crate) forbidden_commands: RegexPolicy,
    pub(crate) secret_paths: SecretPathPolicy,
    pub(crate) skills: SourceSkills,
    pub(crate) claude_settings: Value,
    pub(crate) codex_config: DocumentMut,
    /// The source file of each provider config that is applied to the installed one, by the path
    /// the registry names it with; blank when the source has none.
    pub(crate) provider_configs: BTreeMap<String, ConfigDocument>,
    /// The directories installation replaces from the source, by their path in it.
    pub(crate) directories: BTreeMap<PathBuf, SourceDir>,
    pub(crate) hook_bundles: Vec<HookBundle>,
    pub(crate) external_skills: Vec<ExternalSkill>,
}

impl Source {
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    /// The directory at `path` in the source, when installation reads it.
    pub(crate) fn directory(&self, path: &Path) -> Option<&SourceDir> {
        self.directories.get(path)
    }
}

/// A source directory that is copied whole: where it is and which regular files it holds.
///
/// Symlinks below the directory are not followed, so a copy never reaches outside it; the
/// directory itself may be a symlink.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SourceDir {
    pub(crate) path: PathBuf,
    /// Whether the directory exists; an optional one a profile omits is absent.
    pub(crate) present: bool,
    /// Absolute paths of the regular files, sorted.
    pub(crate) files: Vec<PathBuf>,
}

impl SourceDir {
    pub(crate) fn absent(path: PathBuf) -> Self {
        Self {
            path,
            present: false,
            files: Vec::new(),
        }
    }

    /// The files relative to the directory.
    pub(crate) fn relative_files(&self) -> impl Iterator<Item = &Path> {
        self.files
            .iter()
            .filter_map(|file| file.strip_prefix(&self.path).ok())
    }

    /// Publishes the directory at `target`; an absent one publishes an empty directory.
    pub(crate) fn publish_to(&self, target: PathBuf) -> FileOp {
        publish(self, target)
    }
}

fn publish(directory: &SourceDir, target: PathBuf) -> FileOp {
    if directory.present {
        return FileOp::CopyTree {
            source: directory.path.clone(),
            target,
        };
    }
    FileOp::ReplaceTree {
        target,
        contents: Vec::new(),
    }
}

/// An external hook bundle, read and checked.
#[derive(Debug)]
pub(crate) struct HookBundle {
    pub(crate) name: String,
    pub(crate) root: PathBuf,
    /// The files installed from the bundle, sorted by installed path.
    pub(crate) assets: Vec<HookAsset>,
    /// The bundle's hook files that exist, parsed, by path relative to the bundle.
    pub(crate) hook_files: BTreeMap<String, Value>,
    /// The bundle's Codex config and where it was read from, when it has one.
    pub(crate) codex_config: Option<(PathBuf, DocumentMut)>,
}

/// A file of a bundle and the path, relative to the installation, it is copied to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HookAsset {
    pub(crate) source: PathBuf,
    pub(crate) target: PathBuf,
}
