//! Which guarded filesystem entries cover which paths, so a looser entry never reopens a path a
//! stricter one denies.
//!
//! Codex lets the more specific entry win. A `read` entry for `secrets/key` therefore reopens
//! reads under a `none` entry for `secrets/**`. Every looser entry the harness or the source
//! writes is a concrete path, so the check is only "does this stricter entry cover that one
//! path", never glob against glob. The type enforces it: only a denial can hold a pattern.
//!
//! Paths are compared lexically, per root: `~`, absolute paths and the workspace are separate
//! roots because the home directory and the workspace are unknown at generation time.

use anyhow::{Result, anyhow, bail};

use self::pattern::{Segment, push_segments};

mod pattern;

/// How much of a path Codex lets a command touch; a later variant is stricter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Access {
    Write,
    Read,
    Deny,
}

impl Access {
    /// Parses a Codex access value; `none` is the legacy spelling of `deny`.
    pub(crate) fn parse(text: &str) -> Option<Self> {
        match text {
            "write" => Some(Self::Write),
            "read" => Some(Self::Read),
            "none" | "deny" => Some(Self::Deny),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Root {
    Everything,
    Absolute,
    Home,
    Workspace,
}

/// A normalized path under one root; its segments may be globs.
///
/// `windows` marks a Windows-form path (a drive letter or a UNC prefix): `\` separates its
/// segments and its names compare without regard to case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GuardedPath {
    root: Root,
    windows: bool,
    segments: Vec<Segment>,
}

impl GuardedPath {
    /// The whole filesystem, for `:root`.
    pub(crate) fn everything() -> Self {
        Self::at(Root::Everything, false, Vec::new())
    }

    /// The workspace, for `:workspace_roots`; Windows-form when `root`, the absolute path the
    /// workspace is installed at, is, or when the harness runs on Windows.
    pub(crate) fn workspace(root: Option<&Self>) -> Self {
        Self::workspace_on(root, cfg!(windows))
    }

    fn workspace_on(root: Option<&Self>, host_is_windows: bool) -> Self {
        let windows = host_is_windows || root.is_some_and(|root| root.windows);
        Self::at(Root::Workspace, windows, Vec::new())
    }

    /// Parses a `~/...` or absolute key.
    ///
    /// # Errors
    ///
    /// Returns an error when `text` is neither, or climbs above its root with `..`.
    pub(crate) fn absolute(text: &str) -> Result<Self> {
        Self::absolute_on(text, cfg!(windows))
    }

    /// Every path is Windows-form on a Windows host, where Codex resolves `~` and `/` paths with
    /// Windows rules too; a `~\...` key is Windows-form on any host.
    fn absolute_on(text: &str, host_is_windows: bool) -> Result<Self> {
        let (root, windows, rest) = split_root(text)?;
        let backslash_home = root == Root::Home && rest.starts_with('\\');
        let windows = windows || host_is_windows || backslash_home;
        let mut segments = Vec::new();
        push_segments(&mut segments, rest, windows)
            .ok_or_else(|| anyhow!("guarded path {text:?} climbs above its root"))?;
        Ok(Self::at(root, windows, segments))
    }

    /// The path of `child` under this path.
    ///
    /// # Errors
    ///
    /// Returns an error when `child` climbs above this path with `..`.
    pub(crate) fn join(&self, child: &str) -> Result<Self> {
        let mut segments = self.segments.clone();
        push_segments(&mut segments, child, self.windows)
            .ok_or_else(|| anyhow!("guarded path {child:?} climbs above its scope"))?;
        Ok(Self::at(self.root, self.windows, segments))
    }

    fn at(root: Root, windows: bool, segments: Vec<Segment>) -> Self {
        Self {
            root,
            windows,
            segments,
        }
    }

    fn without_trailing_double_star(mut self) -> Self {
        self.segments
            .pop_if(|segment| *segment == Segment::DoubleStar);
        self
    }

    fn is_concrete(&self) -> bool {
        self.segments
            .iter()
            .all(|segment| matches!(segment, Segment::Literal(_)))
    }

    /// `/` holds the home directory, so it covers `~` entries too; `~` and `/x` stay unrelated
    /// because the home directory is unknown at generation time.
    fn covers_root(&self, root: Root) -> bool {
        let holds_home = root == Root::Home && self.segments.is_empty();
        match self.root {
            Root::Everything => true,
            Root::Absolute => root == Root::Absolute || holds_home,
            Root::Home | Root::Workspace => self.root == root,
        }
    }

    /// Whether this path is, or is an ancestor of, or matches an ancestor of, `path`; with the
    /// `workspace` root, also when it does so for `path` written the other way, absolute or
    /// relative to the workspace.
    fn covers(&self, path: &Self, workspace: Option<&Self>) -> bool {
        self.covers_as_written(path)
            || workspace
                .and_then(|workspace| path.rewritten_for(workspace))
                .is_some_and(|rewritten| self.covers_as_written(&rewritten))
    }

    fn covers_as_written(&self, path: &Self) -> bool {
        let ignore_case = self.windows || path.windows;
        self.covers_root(path.root) && pattern::covers(ignore_case, &self.segments, &path.segments)
    }

    /// The same location for a project-local install, where the workspace is the runtime root:
    /// an absolute path inside `workspace` becomes a workspace path, and a workspace path
    /// becomes an absolute one. `None` when the path is neither.
    fn rewritten_for(&self, workspace: &Self) -> Option<Self> {
        match self.root {
            Root::Workspace => Some(self.below(workspace)),
            Root::Absolute => self.relative_to(workspace),
            Root::Everything | Root::Home => None,
        }
    }

    fn below(&self, workspace: &Self) -> Self {
        let segments = workspace.segments.iter().chain(&self.segments).cloned();
        Self::at(
            Root::Absolute,
            self.windows || workspace.windows,
            segments.collect(),
        )
    }

    fn relative_to(&self, workspace: &Self) -> Option<Self> {
        let inside = self
            .segments
            .get(workspace.segments.len()..)
            .filter(|_| workspace.covers_as_written(self))?;
        Some(Self::at(
            Root::Workspace,
            self.windows || workspace.windows,
            inside.to_vec(),
        ))
    }
}

fn split_root(text: &str) -> Result<(Root, bool, &str)> {
    if let Some(rest) = text.strip_prefix('~') {
        return Ok((Root::Home, false, rest));
    }
    if has_drive_prefix(text) {
        return Ok((Root::Absolute, true, text));
    }
    if let Some(rest) = text.strip_prefix("\\\\") {
        return Ok((Root::Absolute, true, rest));
    }
    match text.strip_prefix('/') {
        Some(rest) => Ok((Root::Absolute, false, rest)),
        None => bail!("guarded path {text:?} must be absolute or start with ~"),
    }
}

/// Whether a table key names an absolute location: `~`, `/...`, a Windows drive path or a UNC
/// path.
pub(crate) fn is_absolute_key(key: &str) -> bool {
    key.starts_with(['/', '~']) || key.starts_with("\\\\") || has_drive_prefix(key)
}

/// `C:/...`, as a runtime root on Windows writes it; the drive stays the first segment.
fn has_drive_prefix(text: &str) -> bool {
    let mut chars = text.chars();
    let drive = chars.next().is_some_and(|ch| ch.is_ascii_alphabetic());
    drive && chars.next() == Some(':') && matches!(chars.next(), Some('/' | '\\'))
}

/// A guarded entry. Only a denial may hold a pattern; a looser entry is one concrete path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum GuardedEntry {
    Deny(GuardedPath),
    Looser { path: GuardedPath, access: Access },
}

impl GuardedEntry {
    /// Builds an entry, or `None` for a looser entry on a pattern, which Codex does not accept
    /// and which cannot be checked for coverage.
    pub(crate) fn new(path: GuardedPath, access: Access) -> Option<Self> {
        match access {
            Access::Deny => Some(Self::Deny(path)),
            _ => Self::looser(path.without_trailing_double_star(), access),
        }
    }

    /// Codex reads a trailing `/**` on a looser entry as the whole subtree, so the entry is the
    /// directory itself, which covers what is below it just the same.
    fn looser(path: GuardedPath, access: Access) -> Option<Self> {
        path.is_concrete().then_some(Self::Looser { path, access })
    }

    fn path(&self) -> &GuardedPath {
        match self {
            Self::Deny(path) | Self::Looser { path, .. } => path,
        }
    }

    pub(crate) fn access(&self) -> Access {
        match self {
            Self::Deny(_) => Access::Deny,
            Self::Looser { access, .. } => *access,
        }
    }
}

/// A pair of entries where the stricter one covers the looser one.
///
/// Codex applies the more specific entry, so the looser entry reopens what the stricter one
/// denies, or, when the looser one is the intended exception, the stricter one is broader than
/// it should be. Either way the entries disagree about the same path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Conflict {
    pub(crate) stricter: usize,
    pub(crate) looser: usize,
}

/// Every pair of `entries` in which the entry covering a looser one most closely is stricter.
///
/// Codex lets the most specific entry win, so a path resolves through the nearest entry that
/// covers it, never through every ancestor. An entry `near` covers `path` most closely when no
/// other entry covers `path` from inside `near`. Entries that cover each other, such as equal
/// paths, are equally near.
///
/// So a denial of `:root` and a grant of `~` leave `~/.codex/hooks.json = "read"` alone: the
/// grant is the nearest entry above it and is looser. The grant itself is a conflict with the
/// denial above it, but it is the author's choice only when the source declares both; that is
/// for the caller to judge.
///
/// With `workspace`, the runtime root of a project-local install, an absolute path inside it and
/// the same path relative to `:workspace_roots` are one location.
pub(crate) fn conflicts(
    entries: &[GuardedEntry],
    workspace: Option<&GuardedPath>,
) -> Vec<Conflict> {
    let relation = Relation { entries, workspace };
    let mut found: Vec<Conflict> = (0..entries.len())
        .flat_map(|looser| relation.conflicts_of(looser))
        .collect();
    found.sort_by_key(|conflict| (conflict.stricter, conflict.looser));
    found
}

struct Relation<'a> {
    entries: &'a [GuardedEntry],
    workspace: Option<&'a GuardedPath>,
}

impl Relation<'_> {
    fn conflicts_of(&self, looser: usize) -> Vec<Conflict> {
        let access = self.access(looser);
        self.nearest_covering(looser)
            .into_iter()
            .filter(|&stricter| self.access(stricter) > access)
            .map(|stricter| Conflict { stricter, looser })
            .collect()
    }

    fn nearest_covering(&self, inner: usize) -> Vec<usize> {
        let covering: Vec<usize> = (0..self.entries.len())
            .filter(|&outer| outer != inner && self.covers(outer, inner))
            .collect();
        covering
            .iter()
            .copied()
            .filter(|&outer| !self.has_nearer(outer, &covering))
            .collect()
    }

    /// Whether another covering entry lies strictly inside `outer`.
    fn has_nearer(&self, outer: usize, covering: &[usize]) -> bool {
        covering
            .iter()
            .any(|&other| other != outer && self.covers(outer, other) && !self.covers(other, outer))
    }

    fn covers(&self, outer: usize, inner: usize) -> bool {
        let entries = self.entries.get(outer).zip(self.entries.get(inner));
        entries.is_some_and(|(outer, inner)| outer.covers(inner, self.workspace))
    }

    fn access(&self, index: usize) -> Access {
        self.entries
            .get(index)
            .map_or(Access::Write, GuardedEntry::access)
    }
}

impl GuardedEntry {
    fn covers(&self, other: &Self, workspace: Option<&GuardedPath>) -> bool {
        self.path().covers(other.path(), workspace)
    }
}

#[cfg(test)]
mod tests;
