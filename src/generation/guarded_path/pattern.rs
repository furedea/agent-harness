//! Path segments and the glob syntax Codex reads in them.
//!
//! Codex compiles each guarded key with `globset`, so a segment follows its rules: `*`, `?`,
//! `[abc]`, `[a-z]`, `[!x]` and `{a,b}`, where a `-` that opens or closes a class is a member
//! of it (`globset` `parse_class`).

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Segment {
    Literal(String),
    DoubleStar,
    Glob(Vec<Token>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Token {
    Star,
    AnyChar,
    Char(char),
    Class {
        negated: bool,
        ranges: Vec<(char, char)>,
    },
    Alternatives(Vec<Vec<Token>>),
}

/// Appends the segments of `text` to `segments`, dropping empty and `.` parts and resolving `..`
/// lexically. `\` separates segments only in a Windows-form path; elsewhere it is a filename
/// character.
pub(super) fn push_segments(segments: &mut Vec<Segment>, text: &str, windows: bool) -> Option<()> {
    let is_separator = |ch: char| ch == '/' || (windows && ch == '\\');
    widen_braces(text, is_separator)
        .split(is_separator)
        .try_for_each(|part| apply_part(segments, part))
}

/// A `{a/b}` group spans segments, so everything from its segment on is widened to `**`, which
/// only ever makes the pattern cover more paths.
fn widen_braces(text: &str, is_separator: impl Fn(char) -> bool) -> String {
    let Some(open) = text.find('{') else {
        return text.to_owned();
    };
    let group = text.get(open..).unwrap_or_default();
    let group = group.split_once('}').map_or(group, |(inside, _)| inside);
    if !group.contains(&is_separator) {
        return text.to_owned();
    }
    let keep = text.get(..open).unwrap_or_default();
    let directory = keep
        .rfind(&is_separator)
        .map_or("", |separator| keep.get(..=separator).unwrap_or(""));
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
        ranges.extend(ranges_from(low, &mut chars));
    }
    ranges
}

/// The range that starts at `low`; a `-` that ends the class is a member of it.
fn ranges_from(
    low: char,
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
) -> Vec<(char, char)> {
    let dashed = chars.next_if_eq(&'-').is_some();
    let high = dashed.then(|| chars.next()).flatten();
    match (dashed, high) {
        (true, None) => vec![(low, low), ('-', '-')],
        (_, high) => vec![(low, high.unwrap_or(low))],
    }
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

/// Whether `pattern` is, or is an ancestor of, or matches an ancestor of, `path`.
pub(super) fn covers(ignore_case: bool, pattern: &[Segment], path: &[Segment]) -> bool {
    let Some((first, rest)) = pattern.split_first() else {
        return true;
    };
    if *first == Segment::DoubleStar {
        let skips_one = path
            .split_first()
            .is_some_and(|(_, tail)| covers(ignore_case, pattern, tail));
        return covers(ignore_case, rest, path) || skips_one;
    }
    let Some((head, tail)) = path.split_first() else {
        return false;
    };
    segment_matches(ignore_case, first, head) && covers(ignore_case, rest, tail)
}

fn segment_matches(ignore_case: bool, pattern: &Segment, concrete: &Segment) -> bool {
    let Segment::Literal(text) = concrete else {
        return false;
    };
    match pattern {
        Segment::Literal(literal) => same_text(ignore_case, literal, text),
        Segment::Glob(tokens) => {
            tokens_match(ignore_case, tokens, &text.chars().collect::<Vec<_>>())
        }
        Segment::DoubleStar => true,
    }
}

fn same_text(ignore_case: bool, left: &str, right: &str) -> bool {
    left == right || (ignore_case && left.to_lowercase() == right.to_lowercase())
}

fn same_char(ignore_case: bool, left: char, right: char) -> bool {
    left == right || (ignore_case && lower(left) == lower(right))
}

fn class_holds(ignore_case: bool, ranges: &[(char, char)], ch: char) -> bool {
    let holds = |ch: char| {
        ranges
            .iter()
            .any(|(low, high)| (*low..=*high).contains(&ch))
    };
    holds(ch) || (ignore_case && (holds(lower(ch)) || holds(upper(ch))))
}

fn tokens_match(ignore_case: bool, tokens: &[Token], text: &[char]) -> bool {
    let Some((first, rest)) = tokens.split_first() else {
        return text.is_empty();
    };
    let after = |tail: &[char]| tokens_match(ignore_case, rest, tail);
    match first {
        Token::Star => (0..=text.len()).any(|skipped| text.get(skipped..).is_some_and(after)),
        Token::AnyChar => text.split_first().is_some_and(|(_, tail)| after(tail)),
        Token::Char(expected) => text
            .split_first()
            .is_some_and(|(ch, tail)| same_char(ignore_case, *ch, *expected) && after(tail)),
        Token::Class { negated, ranges } => text.split_first().is_some_and(|(ch, tail)| {
            class_holds(ignore_case, ranges, *ch) != *negated && after(tail)
        }),
        Token::Alternatives(alternatives) => alternatives.iter().any(|alternative| {
            let joined: Vec<Token> = alternative.iter().chain(rest).cloned().collect();
            tokens_match(ignore_case, &joined, text)
        }),
    }
}

fn lower(ch: char) -> char {
    ch.to_lowercase().next().unwrap_or(ch)
}

fn upper(ch: char) -> char {
    ch.to_uppercase().next().unwrap_or(ch)
}
