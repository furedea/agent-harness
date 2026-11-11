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

    /// The operation with every target under `from` moved to the same place under `to`.
    ///
    /// Sources are unchanged: this reuses one provider's output for another without reading it
    /// back from disk.
    #[must_use]
    pub(crate) fn relocated(self, from: &Path, to: &Path) -> Self {
        let move_target = |target: PathBuf| match target.strip_prefix(from) {
            Ok(relative) => to.join(relative),
            Err(_) => target,
        };
        match self {
            Self::WriteFile {
                target,
                content,
                mode,
            } => Self::WriteFile {
                target: move_target(target),
                content,
                mode,
            },
            Self::CopyFile { source, target } => Self::CopyFile {
                source,
                target: move_target(target),
            },
            Self::CopyTree { source, target } => Self::CopyTree {
                source,
                target: move_target(target),
            },
            Self::ReplaceTree { target, contents } => Self::ReplaceTree {
                target: move_target(target),
                contents: contents
                    .into_iter()
                    .map(|operation| operation.relocated(from, to))
                    .collect(),
            },
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relocation_moves_every_target_under_the_old_root_and_keeps_sources() {
        let operation = FileOp::ReplaceTree {
            target: PathBuf::from("out/.codex/skills"),
            contents: vec![
                FileOp::write("out/.codex/skills/a/SKILL.md", "a"),
                FileOp::CopyTree {
                    source: PathBuf::from("source/skills/a/data"),
                    target: PathBuf::from("out/.codex/skills/a/data"),
                },
            ],
        };

        let moved =
            operation.relocated(Path::new("out/.codex/skills"), Path::new("out/.pi/skills"));

        let expected = FileOp::ReplaceTree {
            target: PathBuf::from("out/.pi/skills"),
            contents: vec![
                FileOp::write("out/.pi/skills/a/SKILL.md", "a"),
                FileOp::CopyTree {
                    source: PathBuf::from("source/skills/a/data"),
                    target: PathBuf::from("out/.pi/skills/a/data"),
                },
            ],
        };
        assert_eq!(moved, expected);
    }

    #[test]
    fn relocation_leaves_a_target_outside_the_old_root_alone() {
        let operation = FileOp::write("elsewhere/file", "x");

        let moved = operation.relocated(Path::new("out/a"), Path::new("out/b"));

        assert_eq!(moved, FileOp::write("elsewhere/file", "x"));
    }
}
