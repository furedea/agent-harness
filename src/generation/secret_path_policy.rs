use std::path::Path;

use anyhow::{Context, Result, ensure};
use serde::Deserialize;

use crate::{
    generation::permission_rule::{CodexFilesystemEntry, PathAccess, PathPattern, PathRule},
    source_model::Source,
};

/// The secret path policy of a source, parsed and validated once.
#[derive(Debug, Deserialize)]
pub(crate) struct SecretPathPolicy {
    version: u64,
    rules: Vec<SecretPathRule>,
}

#[derive(Debug, Deserialize)]
struct SecretPathRule {
    pattern: PathPattern,
    access: Vec<SecretPathAccess>,
}

impl SecretPathRule {
    fn denies_read(&self) -> bool {
        self.access
            .iter()
            .any(|access| matches!(access, SecretPathAccess::Read))
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SecretPathAccess {
    Read,
    Write,
}

impl From<SecretPathAccess> for PathAccess {
    fn from(access: SecretPathAccess) -> Self {
        match access {
            SecretPathAccess::Read => Self::Read,
            SecretPathAccess::Write => Self::Write,
        }
    }
}

impl SecretPathPolicy {
    /// Parses and validates the policy `text` read from `path`.
    ///
    /// # Errors
    ///
    /// Returns an error naming `path` when the text is not a valid secret path policy.
    pub(crate) fn parse(path: &Path, text: &str) -> Result<Self> {
        let policy: Self = serde_json::from_str(text)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        validate_policy(&policy)?;
        Ok(policy)
    }
}

pub(crate) fn claude_deny_permissions(source: &Source) -> Vec<String> {
    source
        .secret_paths
        .rules
        .iter()
        .flat_map(|rule| {
            rule.access
                .iter()
                .map(|&access| PathRule::new(rule.pattern.clone(), access.into()).claude())
        })
        .collect()
}

/// The Codex filesystem entries the policy requires, one per rule that denies reads.
///
/// A rule that only denies writes has no Codex entry: Codex's permissions apply to every process
/// the agent starts, and the harness scripts write the paths such rules guard. Hooks keep
/// enforcing them.
///
/// # Errors
///
/// Returns an error when a rule's pattern is not a path Codex can address.
pub(crate) fn codex_filesystem_entries(source: &Source) -> Result<Vec<CodexFilesystemEntry>> {
    source
        .secret_paths
        .rules
        .iter()
        .filter(|rule| rule.denies_read())
        .map(|rule| rule.pattern.codex_denied_read())
        .collect()
}

fn validate_policy(policy: &SecretPathPolicy) -> Result<()> {
    ensure!(
        policy.version == 1,
        "unsupported secret path policy version: {}",
        policy.version
    );

    for rule in &policy.rules {
        ensure!(
            !rule.access.is_empty(),
            "secret path policy access list must not be empty"
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use crate::layout::SourceLayout;
    use crate::shell::fixtures::load_source;

    #[test]
    fn claude_deny_permissions_render_from_secret_path_policy() -> Result<()> {
        let root = test_root("claude_deny_permissions_render_from_secret_path_policy")?;
        write_policy(
            &root,
            r#"{
  "version": 1,
  "rules": [
    {"pattern": ".env*", "access": ["read", "write"]},
    {"pattern": "~/.ssh/**", "access": ["read"]}
  ]
}
"#,
        )?;

        let permissions = claude_deny_permissions(&load_source(&root)?);

        assert_eq!(
            permissions,
            vec![
                "Read(.env*)".to_string(),
                "Edit(.env*)".to_string(),
                "Read(~/.ssh/**)".to_string(),
            ],
        );

        crate::shell::fixtures::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn invalid_policy_version_is_rejected() -> Result<()> {
        let root = test_root("invalid_policy_version_is_rejected")?;
        write_policy(&root, r#"{"version":2,"rules":[]}"#)?;

        let error = format!("{:#}", load_source(&root).unwrap_err());

        assert!(error.contains("unsupported secret path policy version"));

        crate::shell::fixtures::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn patterns_with_parentheses_are_rejected() -> Result<()> {
        let root = test_root("patterns_with_parentheses_are_rejected")?;
        write_policy(
            &root,
            r#"{"version":1,"rules":[{"pattern":"a)b","access":["read"]}]}"#,
        )?;

        let error = format!("{:#}", load_source(&root).unwrap_err());

        assert!(error.contains("parentheses"));

        crate::shell::fixtures::remove_dir_all(root)?;
        Ok(())
    }

    fn test_root(name: &str) -> Result<PathBuf> {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let root = std::env::temp_dir().join(format!("agent-harness-{name}-{nanos}"));
        crate::shell::fixtures::create_dir_all(&root)?;
        Ok(root)
    }

    fn write_policy(source: &Path, content: &str) -> Result<()> {
        let path = SourceLayout::new(source).secret_path_policy();
        path.parent()
            .map(crate::shell::fixtures::create_dir_all)
            .transpose()?;
        crate::shell::fixtures::write(path, content)?;
        Ok(())
    }
}
