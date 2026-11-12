//! Validated permission rule values and their per-provider renderers.
//!
//! Rules are parsed once at the source boundary so every provider renders the same meaning.

use anyhow::{Result, ensure};

use serde::{Deserialize, Serialize};

/// A command prefix split into tokens, none of which is empty or contains whitespace.
///
/// Whitespace inside a token would read as several tokens in providers that join with spaces and
/// as one token in providers that keep a list, so it is rejected when parsing.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(try_from = "Vec<String>", into = "Vec<String>")]
pub(crate) struct PrefixRule(Vec<String>);

impl PrefixRule {
    pub(crate) fn tokens(&self) -> &[String] {
        &self.0
    }

    pub(crate) fn claude(&self) -> String {
        format!("Bash({}:*)", self.0.join(" "))
    }

    pub(crate) fn devin(&self) -> String {
        format!("Exec({})", self.0.join(" "))
    }
}

impl TryFrom<Vec<String>> for PrefixRule {
    type Error = anyhow::Error;

    fn try_from(tokens: Vec<String>) -> Result<Self> {
        ensure!(!tokens.is_empty(), "prefix must be non-empty");
        tokens.iter().try_for_each(|token| validate_token(token))?;
        Ok(Self(tokens))
    }
}

fn validate_token(token: &str) -> Result<()> {
    ensure!(!token.is_empty(), "prefix contains an empty segment");
    ensure!(
        !token.chars().any(char::is_whitespace),
        "prefix segment {token:?} must not contain whitespace"
    );
    Ok(())
}

impl From<PrefixRule> for Vec<String> {
    fn from(rule: PrefixRule) -> Self {
        rule.0
    }
}

/// A path pattern that can sit inside a tool permission such as `Read(...)`.
///
/// Parentheses end the tool argument early, so they are rejected when parsing.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")]
pub(crate) struct PathPattern(String);

impl PathPattern {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    /// Renders the pattern as a Codex `[permissions.<profile>.filesystem]` entry denying all
    /// access (`none`), for a rule that denies reads.
    ///
    /// Codex accepts a glob key only with `none`, and only as an absolute or `~/` key or as a
    /// subpath under `:workspace_roots`, so a pattern it cannot express is an error instead of
    /// a config Codex would reject. A rule that only denies writes has no Codex entry: Codex's
    /// permissions apply to every process the agent starts, and the harness's own scripts write
    /// those paths.
    ///
    /// # Errors
    ///
    /// Returns an error when the pattern is not a path Codex can address.
    pub(crate) fn codex_denied_read(&self) -> Result<CodexFilesystemEntry> {
        let (scope, path) = codex_location(self.as_str())?;
        Ok(CodexFilesystemEntry {
            scope,
            path,
            access: CodexAccess::None,
        })
    }
}

impl TryFrom<String> for PathPattern {
    type Error = anyhow::Error;

    fn try_from(pattern: String) -> Result<Self> {
        ensure!(!pattern.trim().is_empty(), "path pattern must not be empty");
        ensure!(
            !pattern.contains(['(', ')']),
            "path pattern {pattern:?} must not contain parentheses"
        );
        Ok(Self(pattern))
    }
}

/// Which file operation a path rule governs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PathAccess {
    Read,
    Write,
}

/// A path pattern paired with the access it governs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PathRule {
    pattern: PathPattern,
    access: PathAccess,
}

impl PathRule {
    pub(crate) fn new(pattern: PathPattern, access: PathAccess) -> Self {
        Self { pattern, access }
    }

    /// Denies writes to a path derived from the install root or synced file names.
    ///
    /// Such paths are not authored policy, so parentheses (for example in a root directory
    /// name) are kept as before instead of failing generation.
    pub(crate) fn write_installed_path(path: String) -> Self {
        Self::new(PathPattern(path), PathAccess::Write)
    }

    pub(crate) fn claude(&self) -> String {
        let tool = match self.access {
            PathAccess::Read => "Read",
            PathAccess::Write => "Edit",
        };
        format!("{tool}({})", self.pattern.as_str())
    }
}

/// How much of a path Codex lets a command touch; a later variant is stricter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum CodexAccess {
    Read,
    None,
}

impl CodexAccess {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::None => "none",
        }
    }
}

/// Where Codex resolves a filesystem entry's path from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CodexScope {
    Absolute,
    WorkspaceRoots,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CodexFilesystemEntry {
    pub(crate) scope: CodexScope,
    pub(crate) path: String,
    pub(crate) access: CodexAccess,
}

impl CodexFilesystemEntry {
    /// A protected installed path: readable, never writable.
    pub(crate) fn protected(path: String) -> Self {
        Self {
            scope: CodexScope::Absolute,
            path,
            access: CodexAccess::Read,
        }
    }
}

/// Claude writes a project-relative path as `./secrets/**`, and a directory may end in `/`; both
/// name the same path without them.
fn normalized_pattern(path: &str) -> &str {
    let trimmed = path.trim_end_matches('/');
    let trimmed = if trimmed.is_empty() { path } else { trimmed };
    trimmed.strip_prefix("./").unwrap_or(trimmed)
}

/// Resolves a Claude-style pattern to where and how Codex matches it.
///
/// Claude reads `//x` as absolute, `/x` as relative to the project, and a slash-free relative
/// pattern as matching at any depth. Codex only has absolute and `~/` keys and
/// `:workspace_roots` subpaths, so `/x` is rejected as ambiguous and a slash-free pattern gets a
/// `**/` prefix so it still matches at any depth.
fn codex_location(path: &str) -> Result<(CodexScope, String)> {
    ensure!(
        !path.contains(['{', '}']),
        "Codex does not expand braces, so {path:?} would match a literal name; list each path as its own rule"
    );
    let path = normalized_pattern(path);
    if let Some(absolute) = path.strip_prefix("//") {
        return Ok((CodexScope::Absolute, format!("/{absolute}")));
    }
    ensure!(
        !path.starts_with('/'),
        "{path:?} is ambiguous: write // for an absolute path or drop the leading / for a project path"
    );
    if path == "~" || path.starts_with("~/") {
        return Ok((CodexScope::Absolute, path.to_owned()));
    }
    ensure!(
        !path.split('/').any(|part| matches!(part, "" | "." | "..")),
        "Codex needs {path:?} to be absolute, start with ~/, or be a descendant of the workspace without . or .. parts"
    );
    if path.contains('/') {
        return Ok((CodexScope::WorkspaceRoots, path.to_owned()));
    }
    Ok((CodexScope::WorkspaceRoots, format!("**/{path}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prefix(tokens: &[&str]) -> Result<PrefixRule> {
        PrefixRule::try_from(tokens.iter().map(|t| (*t).to_owned()).collect::<Vec<_>>())
    }

    #[test]
    fn prefix_renders_per_provider() -> Result<()> {
        let rule = prefix(&["git", "push"])?;

        assert_eq!(rule.claude(), "Bash(git push:*)");
        assert_eq!(rule.devin(), "Exec(git push)");
        assert_eq!(rule.tokens(), ["git", "push"]);
        Ok(())
    }

    #[test]
    fn prefix_rejects_empty_list_and_empty_segment() {
        assert!(prefix(&[]).is_err());
        assert!(prefix(&["git", ""]).is_err());
    }

    #[test]
    fn prefix_rejects_whitespace_inside_a_segment() {
        let error = prefix(&["rm -rf"]).unwrap_err().to_string();

        assert!(error.contains("whitespace"));
        assert!(prefix(&["git\tpush"]).is_err());
    }

    #[test]
    fn prefix_deserializes_and_serializes_as_a_token_list() -> Result<()> {
        let rule: PrefixRule = serde_json::from_str(r#"["git","push"]"#)?;

        assert_eq!(serde_json::to_string(&rule)?, r#"["git","push"]"#);
        assert!(serde_json::from_str::<PrefixRule>(r#"["rm -rf"]"#).is_err());
        Ok(())
    }

    #[test]
    fn path_pattern_rejects_empty_and_parentheses() {
        assert!(PathPattern::try_from(" ".to_owned()).is_err());
        assert!(PathPattern::try_from("a(b".to_owned()).is_err());
        assert!(PathPattern::try_from("a)b".to_owned()).is_err());
        assert!(PathPattern::try_from("~/.ssh/**".to_owned()).is_ok());
    }

    #[test]
    fn installed_paths_keep_parentheses() {
        let rule = PathRule::write_installed_path("/Users/me/Work (old)/x".to_owned());

        assert_eq!(rule.claude(), "Edit(/Users/me/Work (old)/x)");
    }

    #[test]
    fn path_rule_renders_read_and_edit_for_claude() -> Result<()> {
        let pattern = PathPattern::try_from(".env*".to_owned())?;

        assert_eq!(
            PathRule::new(pattern.clone(), PathAccess::Read).claude(),
            "Read(.env*)"
        );
        assert_eq!(
            PathRule::new(pattern, PathAccess::Write).claude(),
            "Edit(.env*)"
        );
        Ok(())
    }

    fn codex_entry(pattern: &str) -> Result<CodexFilesystemEntry> {
        PathPattern::try_from(pattern.to_owned())?.codex_denied_read()
    }

    #[test]
    fn codex_denied_read_is_none() -> Result<()> {
        let entry = codex_entry("~/.ssh/**")?;

        assert_eq!(
            (entry.scope, entry.path.as_str(), entry.access),
            (CodexScope::Absolute, "~/.ssh/**", CodexAccess::None)
        );
        assert_eq!(codex_entry("//etc/hosts")?.path, "/etc/hosts");
        Ok(())
    }

    #[test]
    fn codex_relative_patterns_scope_to_the_workspace() -> Result<()> {
        let nested = codex_entry("secrets/**")?;
        let any_depth = codex_entry(".env*")?;

        assert_eq!(nested.scope, CodexScope::WorkspaceRoots);
        assert_eq!(nested.path, "secrets/**");
        assert_eq!(any_depth.path, "**/.env*");
        assert_eq!(codex_entry("./secrets/**")?.path, "secrets/**");
        assert_eq!(codex_entry("secrets/")?.path, "**/secrets");
        assert_eq!(codex_entry("./.env")?.path, "**/.env");
        assert!(codex_entry("secrets/{prod,staging}.json").is_err());
        assert!(codex_entry("../x").is_err());
        assert!(codex_entry("/x").is_err());
        Ok(())
    }
}
