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

#[derive(Debug, Clone, PartialEq, Eq)]
enum Segment {
    Literal(String),
    DoubleStar,
    Glob(Vec<Token>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Star,
    AnyChar,
    Char(char),
}

/// A normalized path under one root; its segments may be globs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GuardedPath {
    root: Root,
    segments: Vec<Segment>,
}

impl GuardedPath {
    /// The whole filesystem, for `:root`.
    pub(crate) fn everything() -> Self {
        Self::at(Root::Everything, Vec::new())
    }

    pub(crate) fn workspace() -> Self {
        Self::at(Root::Workspace, Vec::new())
    }

    /// Parses a `~/...` or absolute key.
    ///
    /// # Errors
    ///
    /// Returns an error when `text` is neither, or climbs above its root with `..`.
    pub(crate) fn absolute(text: &str) -> Result<Self> {
        let (root, rest) = split_root(text)?;
        let mut segments = Vec::new();
        push_segments(&mut segments, rest)
            .ok_or_else(|| anyhow!("guarded path {text:?} climbs above its root"))?;
        Ok(Self::at(root, segments))
    }

    /// The path of `child` under this path.
    ///
    /// # Errors
    ///
    /// Returns an error when `child` climbs above this path with `..`.
    pub(crate) fn join(&self, child: &str) -> Result<Self> {
        let mut segments = self.segments.clone();
        push_segments(&mut segments, child)
            .ok_or_else(|| anyhow!("guarded path {child:?} climbs above its scope"))?;
        Ok(Self::at(self.root, segments))
    }

    fn at(root: Root, segments: Vec<Segment>) -> Self {
        Self { root, segments }
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

    /// Whether this path is, or is an ancestor of, or matches an ancestor of, `path`.
    fn covers(&self, path: &Self) -> bool {
        (self.root == Root::Everything || self.root == path.root)
            && covers_segments(&self.segments, &path.segments)
    }
}

fn split_root(text: &str) -> Result<(Root, &str)> {
    if let Some(rest) = text.strip_prefix('~') {
        return Ok((Root::Home, rest));
    }
    if has_drive_prefix(text) {
        return Ok((Root::Absolute, text));
    }
    match text.strip_prefix('/') {
        Some(rest) => Ok((Root::Absolute, rest)),
        None => bail!("guarded path {text:?} must be absolute or start with ~"),
    }
}

/// Whether a table key names an absolute location: `~`, `/...` or a Windows drive path.
pub(crate) fn is_absolute_key(key: &str) -> bool {
    key.starts_with(['/', '~']) || has_drive_prefix(key)
}

/// `C:/...`, as a runtime root on Windows writes it; the drive stays the first segment.
fn has_drive_prefix(text: &str) -> bool {
    let mut chars = text.chars();
    let drive = chars.next().is_some_and(|ch| ch.is_ascii_alphabetic());
    drive && chars.next() == Some(':') && matches!(chars.next(), Some('/' | '\\'))
}

/// Splits `text` into segments, dropping empty and `.` parts and resolving `..` lexically.
fn push_segments(segments: &mut Vec<Segment>, text: &str) -> Option<()> {
    widen_braces(text)
        .split('/')
        .try_for_each(|part| apply_part(segments, part))
}

/// A `{a/b}` group spans segments, so everything from its segment on is widened to `**`, which
/// only ever makes the pattern cover more paths.
fn widen_braces(text: &str) -> String {
    let Some(open) = text.find('{') else {
        return text.to_owned();
    };
    let group = text.get(open..).unwrap_or_default();
    let group = group.split_once('}').map_or(group, |(inside, _)| inside);
    if !group.contains('/') {
        return text.to_owned();
    }
    let keep = text.get(..open).unwrap_or_default();
    let directory = keep
        .rfind('/')
        .map_or("", |slash| keep.get(..=slash).unwrap_or(""));
    format!("{directory}**")
}

fn apply_part(segments: &mut Vec<Segment>, part: &str) -> Option<()> {
    match part {
        "" | "." => Some(()),
        ".." => segments.pop().map(drop),
        _ => Some(()).inspect(|()| segments.push(segment_of(part))),
    }
}

fn segment_of(part: &str) -> Segment {
    if part == "**" {
        return Segment::DoubleStar;
    }
    if !part.contains(['*', '?', '[', '{']) {
        return Segment::Literal(part.to_owned());
    }
    Segment::Glob(tokens_of(part))
}

/// Classes and alternations are widened to what they could match, which only ever makes the
/// check cover more paths and so deny more.
fn tokens_of(part: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut chars = part.chars();
    while let Some(ch) = chars.next() {
        let token = match ch {
            '*' => Token::Star,
            '?' => Token::AnyChar,
            '[' => skip_group(&mut chars, ']', Token::AnyChar),
            '{' => skip_group(&mut chars, '}', Token::Star),
            _ => Token::Char(ch),
        };
        push_token(&mut tokens, token);
    }
    tokens
}

fn push_token(tokens: &mut Vec<Token>, token: Token) {
    let repeats_star = token == Token::Star && tokens.last() == Some(&Token::Star);
    if !repeats_star {
        tokens.push(token);
    }
}

fn skip_group(chars: &mut std::str::Chars<'_>, close: char, widened: Token) -> Token {
    chars.by_ref().find(|&ch| ch == close);
    widened
}

fn covers_segments(pattern: &[Segment], path: &[Segment]) -> bool {
    let Some((first, rest)) = pattern.split_first() else {
        return true;
    };
    if *first == Segment::DoubleStar {
        let skips_one = path
            .split_first()
            .is_some_and(|(_, tail)| covers_segments(pattern, tail));
        return covers_segments(rest, path) || skips_one;
    }
    let Some((head, tail)) = path.split_first() else {
        return false;
    };
    segment_matches(first, head) && covers_segments(rest, tail)
}

fn segment_matches(pattern: &Segment, concrete: &Segment) -> bool {
    let Segment::Literal(text) = concrete else {
        return false;
    };
    match pattern {
        Segment::Literal(literal) => literal == text,
        Segment::Glob(tokens) => tokens_match(tokens, &text.chars().collect::<Vec<_>>()),
        Segment::DoubleStar => true,
    }
}

fn tokens_match(tokens: &[Token], text: &[char]) -> bool {
    let Some((first, rest)) = tokens.split_first() else {
        return text.is_empty();
    };
    match first {
        Token::Star => (0..=text.len()).any(|skipped| {
            text.get(skipped..)
                .is_some_and(|tail| tokens_match(rest, tail))
        }),
        Token::AnyChar => text
            .split_first()
            .is_some_and(|(_, tail)| tokens_match(rest, tail)),
        Token::Char(expected) => text
            .split_first()
            .is_some_and(|(ch, tail)| ch == expected && tokens_match(rest, tail)),
    }
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

/// Every pair of `entries` in which one entry covers a looser one, by index.
pub(crate) fn conflicts(entries: &[GuardedEntry]) -> Vec<Conflict> {
    let mut found = Vec::new();
    for (stricter, outer) in entries.iter().enumerate() {
        found.extend(
            entries
                .iter()
                .enumerate()
                .filter(|(_, inner)| outer.access() > inner.access() && outer.covers(inner))
                .map(|(looser, _)| Conflict { stricter, looser }),
        );
    }
    found
}

impl GuardedEntry {
    fn covers(&self, other: &Self) -> bool {
        self.path().covers(other.path())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deny(path: &str) -> Result<GuardedEntry> {
        Ok(GuardedEntry::Deny(GuardedPath::absolute(path)?))
    }

    fn read(path: &str) -> Result<GuardedEntry> {
        let entry = GuardedEntry::new(GuardedPath::absolute(path)?, Access::Read);
        entry.ok_or_else(|| anyhow::anyhow!("{path} is not concrete"))
    }

    /// The (stricter, looser) index pairs.
    fn pairs(entries: &[GuardedEntry]) -> Vec<(usize, usize)> {
        conflicts(entries)
            .into_iter()
            .map(|conflict| (conflict.stricter, conflict.looser))
            .collect()
    }

    #[test]
    fn a_looser_entry_cannot_be_built_from_a_pattern() -> Result<()> {
        let path = GuardedPath::absolute("~/.aws/*")?;

        assert!(GuardedEntry::new(path.clone(), Access::Read).is_none());
        assert!(GuardedEntry::new(path, Access::Deny).is_some());
        Ok(())
    }

    #[test]
    fn an_ancestor_directory_covers_a_path_on_component_boundaries() -> Result<()> {
        let entries = [
            deny("~/.aws")?,
            read("~/.aws/credentials")?,
            read("~/.awsome/x")?,
        ];

        assert_eq!(pairs(&entries), [(0, 1)]);
        Ok(())
    }

    #[test]
    fn an_equal_path_with_a_different_access_conflicts() -> Result<()> {
        let entries = [deny("~/.aws/credentials")?, read("~/.aws/credentials")?];

        assert_eq!(pairs(&entries), [(0, 1)]);
        Ok(())
    }

    #[test]
    fn a_glob_covers_the_paths_it_matches() -> Result<()> {
        let entries = [
            deny("~/.aws/**")?,
            deny("/work/*/secret")?,
            read("~/.aws/credentials")?,
            read("/work/a/secret/key")?,
            read("/work/a/b/secret")?,
        ];

        assert_eq!(pairs(&entries), [(0, 2), (1, 3)]);
        Ok(())
    }

    #[test]
    fn double_star_matches_any_depth_and_classes_are_widened() -> Result<()> {
        let entries = [
            deny("/w/**/.env*")?,
            deny("/w/[ab]x")?,
            read("/w/a/b/.env.local")?,
            read("/w/cx")?,
            read("/w/bx/f")?,
        ];

        // `[ab]` is widened to any one character, so `/w/cx` is covered too.
        assert_eq!(pairs(&entries), [(0, 2), (1, 3), (1, 4)]);
        Ok(())
    }

    #[test]
    fn a_stricter_entry_below_a_looser_one_and_equal_accesses_do_not_conflict() -> Result<()> {
        let entries = [
            read("~/.config")?,
            deny("~/.config/gh")?,
            deny("~/.config/gh/hosts")?,
        ];

        assert_eq!(pairs(&entries), []);
        Ok(())
    }

    #[test]
    fn paths_are_normalized_before_they_are_compared() -> Result<()> {
        let entries = [deny("~/.config/../.aws//")?, read("~/./.aws/credentials")?];

        assert_eq!(pairs(&entries), [(0, 1)]);
        Ok(())
    }

    #[test]
    fn a_brace_group_spanning_a_slash_is_widened_to_everything_below() -> Result<()> {
        let entries = [
            deny("/w/{a/b,c}")?,
            read("/w/a/b")?,
            read("/w/z/deep")?,
            read("/v/a")?,
        ];

        assert_eq!(pairs(&entries), [(0, 1), (0, 2)]);
        Ok(())
    }

    #[test]
    fn a_looser_subtree_entry_is_the_directory_and_a_drive_path_is_absolute() -> Result<()> {
        let subtree = GuardedEntry::new(GuardedPath::absolute("~/.ssh/**")?, Access::Read);
        let entries = [deny("~/.ssh/**")?, subtree.expect("a subtree is concrete")];

        assert_eq!(pairs(&entries), [(0, 1)]);
        assert!(GuardedPath::absolute("C:/proj/.claude/hooks/guard.sh").is_ok());
        assert!(is_absolute_key("C:/work") && !is_absolute_key("work/C:"));
        Ok(())
    }

    #[test]
    fn a_path_cannot_climb_above_its_root() {
        assert!(GuardedPath::absolute("~/../x").is_err());
        assert!(GuardedPath::workspace().join("a/../../x").is_err());
    }

    #[test]
    fn roots_do_not_cover_each_other_except_everything() -> Result<()> {
        let workspace = GuardedEntry::Deny(GuardedPath::workspace().join("secrets")?);
        let home = read("~/secrets/x")?;
        let everything = GuardedEntry::Deny(GuardedPath::everything());

        assert_eq!(pairs(&[workspace, home.clone()]), []);
        assert_eq!(pairs(&[everything, home]), [(0, 1)]);
        Ok(())
    }
}
