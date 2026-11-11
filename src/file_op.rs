#![forbid(clippy::disallowed_methods, clippy::disallowed_types)]

use std::path::{Path, PathBuf};

/// Permissions of the files an operation creates.
///
/// Only secrets are private. Everything else is created with the process's default mode, so a
/// generated config is as readable as any file the user makes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FileMode {
    /// The process's default creation mode (`0666` less the umask).
    Standard,
    /// Owner read/write only (`0600`), and `0700` for the directories created on its way.
    Private,
    /// Exactly these permission bits, whatever the umask, to reproduce a packaged file.
    Bits(u32),
}

/// One change to the filesystem, described as a value.
///
/// Computation returns these instead of writing; only the executor in `shell` applies them.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum FileOp {
    /// Creates or replaces a file with `content`.
    WriteFile {
        target: PathBuf,
        content: Vec<u8>,
        mode: FileMode,
    },
    /// Creates or replaces a file with a copy of `source`, keeping its permission bits.
    CopyFile { source: PathBuf, target: PathBuf },
    /// Replaces a directory with a copy of the regular files under `source`.
    CopyTree { source: PathBuf, target: PathBuf },
    /// Replaces a directory with one that holds exactly `contents`, whose targets lie under it.
    ReplaceTree {
        target: PathBuf,
        contents: Vec<FileOp>,
    },
}

impl FileOp {
    /// A file with the standard mode.
    pub(crate) fn write(target: impl Into<PathBuf>, content: impl Into<Vec<u8>>) -> Self {
        Self::WriteFile {
            target: target.into(),
            content: content.into(),
            mode: FileMode::Standard,
        }
    }

    /// A pretty-printed JSON file with a trailing newline.
    ///
    /// # Errors
    ///
    /// Returns an error when `value` cannot be serialized.
    pub(crate) fn write_json<T>(target: impl Into<PathBuf>, value: &T) -> anyhow::Result<Self>
    where
        T: serde::Serialize + ?Sized,
    {
        let content = serde_json::to_string_pretty(value)? + "\n";
        Ok(Self::write(target, content))
    }

    pub(crate) fn target(&self) -> &Path {
        match self {
            Self::WriteFile { target, .. }
            | Self::CopyFile { target, .. }
            | Self::CopyTree { target, .. }
            | Self::ReplaceTree { target, .. } => target,
        }
    }
}
