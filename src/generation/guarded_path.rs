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
    Class {
        negated: bool,
        ranges: Vec<(char, char)>,
    },
    Alternatives(Vec<Vec<Token>>),
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

    /// Whether this path is, or is an ancestor of, or matches an ancestor of, `path`.
    fn covers(&self, path: &Self) -> bool {
        self.covers_root(path.root) && covers_segments(&self.segments, &path.segments)
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

fn tokens_of(part: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut rest = part;
    while let Some(ch) = rest.chars().next() {
        let tail = rest.get(ch.len_utf8()..).unwrap_or("");
        let (token, remaining) = match ch {
            '*' => (Token::Star, tail),
            '?' => (Token::AnyChar, tail),
            '[' => class_token(tail).unwrap_or((Token::Char(ch), tail)),
            '{' => alternatives_token(tail).unwrap_or((Token::Char(ch), tail)),
            _ => (Token::Char(ch), tail),
        };
        push_token(&mut tokens, token);
        rest = remaining;
    }
    tokens
}

/// `[abc]`, `[a-z]` and `[!x]`; `None` when the class is not closed, so `[` is a plain character.
fn class_token(tail: &str) -> Option<(Token, &str)> {
    let (inside, after) = tail.split_once(']')?;
    let negated = inside.starts_with(['!', '^']);
    let body = inside.strip_prefix(['!', '^']).unwrap_or(inside);
    Some((
        Token::Class {
            negated,
            ranges: ranges_of(body),
        },
        after,
    ))
}

fn ranges_of(body: &str) -> Vec<(char, char)> {
    let mut ranges = Vec::new();
    let mut chars = body.chars().peekable();
    while let Some(low) = chars.next() {
        let high = take_range_end(&mut chars).unwrap_or(low);
        ranges.push((low, high));
    }
    ranges
}

/// The `z` of `a-z`, consuming the `-z`; `None` when `-` is last or does not follow a range.
fn take_range_end(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Option<char> {
    chars.next_if_eq(&'-')?;
    chars.next()
}

/// `{a,b}`; `None` when the group is not closed, so `{` is a plain character.
fn alternatives_token(tail: &str) -> Option<(Token, &str)> {
    let (inside, after) = tail.split_once('}')?;
    Some((
        Token::Alternatives(inside.split(',').map(tokens_of).collect()),
        after,
    ))
}

fn push_token(tokens: &mut Vec<Token>, token: Token) {
    let repeats_star = token == Token::Star && tokens.last() == Some(&Token::Star);
    if !repeats_star {
        tokens.push(token);
    }
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
        Token::Class { negated, ranges } => text.split_first().is_some_and(|(ch, tail)| {
            let inside = ranges.iter().any(|(low, high)| (low..=high).contains(&ch));
            inside != *negated && tokens_match(rest, tail)
        }),
        Token::Alternatives(alternatives) => alternatives.iter().any(|alternative| {
            let joined: Vec<Token> = alternative.iter().chain(rest).cloned().collect();
            tokens_match(&joined, text)
        }),
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

/// Every pair of `entries` in which one entry covers a looser one that no entry in between
/// decides.
///
/// Codex uses the most specific matching entry. A third entry that is covered by the stricter one,
/// covers the looser one, and has the looser one's access is an explicit exception, so the looser
/// entry resolves through it and nothing is reopened.
pub(crate) fn conflicts(entries: &[GuardedEntry]) -> Vec<Conflict> {
    let mut found = Vec::new();
    for (stricter, outer) in entries.iter().enumerate() {
        found.extend(
            entries
                .iter()
                .enumerate()
                .filter(|(_, inner)| outer.access() > inner.access() && outer.covers(inner))
                .filter(|(_, inner)| !has_exception_between(entries, outer, inner))
                .map(|(looser, _)| Conflict { stricter, looser }),
        );
    }
    found
}

fn has_exception_between(
    entries: &[GuardedEntry],
    outer: &GuardedEntry,
    inner: &GuardedEntry,
) -> bool {
    entries.iter().any(|between| {
        between != outer
            && between != inner
            && between.access() == inner.access()
            && outer.covers(between)
            && !between.covers(outer)
            && between.covers(inner)
    })
}

impl GuardedEntry {
    fn covers(&self, other: &Self) -> bool {
        self.path().covers(other.path())
    }
}

#[cfg(test)]
mod tests;
