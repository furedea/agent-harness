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
}
