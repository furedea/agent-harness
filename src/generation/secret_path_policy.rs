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
        self.denies(PathAccess::Read)
    }

    fn denies(&self, wanted: PathAccess) -> bool {
        self.access
            .iter()
            .any(|&access| PathAccess::from(access) == wanted)
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

/// The paths the policy requires for Antigravity under `access`, one per rule that denies that
/// access and names a concrete location.
///
/// `agy` rejects a glob in a path rule and takes absolute paths only, so a rule converts only when
/// it names one location outside the project: `//x` becomes `/x`, `~/x` stays for the sync to
/// expand, and a trailing `/**` names the directory itself. Any other glob, and any project
/// relative pattern, has no `agy` form; the `PreToolUse` hook keeps enforcing those.
pub(crate) fn antigravity_denied_paths(source: &Source, access: PathAccess) -> Vec<String> {
    source
        .secret_paths
        .rules
        .iter()
        .filter(|rule| rule.denies(access))
        .filter_map(|rule| antigravity_path(rule.pattern.as_str()))
        .collect()
}

fn antigravity_path(pattern: &str) -> Option<String> {
    let trimmed = pattern.trim_end_matches('/');
    let path = trimmed.strip_suffix("/**").unwrap_or(trimmed);
    if path.trim_end_matches('/').is_empty() && pattern.starts_with("//") {
        return Some("/".to_owned());
    }
    let is_concrete = !path.contains(['*', '?', '[', ']', '{', '}', '!', '\\']);
    let location = match path.strip_prefix("//") {
        Some(absolute) => Some(format!("/{absolute}")),
        None => (path == "~" || path.starts_with("~/")).then(|| path.to_owned()),
    };
    location.filter(|_| is_concrete && !path.is_empty())
}

/// A relative pattern with a `/` other than a trailing one is read at the root by gitignore and
/// Codex, and at the working directory by Claude. Only `**/` says "any depth".
fn ensure_unambiguous(pattern: &PathPattern) -> Result<()> {
    let text = pattern.as_str();
    let body = text.trim_end_matches('/');
    let suggestion = body.strip_prefix("./").unwrap_or(body);
    let is_anchored_elsewhere = text.starts_with('/') || text.starts_with("~/") || body == "~";
    ensure!(
        is_anchored_elsewhere || !body.contains('/') || body.starts_with("**/"),
        "secret path pattern {text:?} is ambiguous: gitignore and Codex anchor it at the repository root, and Claude anchors it at the working directory; write \"**/{suggestion}\" to match at any depth"
    );
    Ok(())
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
            "secret path policy access list must not be empty for pattern {:?}",
            rule.pattern.as_str()
        );
        ensure_unambiguous(&rule.pattern)?;
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
    fn ambiguous_relative_patterns_are_rejected_when_the_policy_is_loaded() -> Result<()> {
        ["secrets/**", "config/x.json", "./secrets/**", "src/keys/*"]
            .into_iter()
            .try_for_each(assert_ambiguous_pattern_rejected)
    }

    fn assert_ambiguous_pattern_rejected(pattern: &str) -> Result<()> {
        let root = test_root("ambiguous_relative_patterns_are_rejected")?;
        write_policy(
            &root,
            &format!(r#"{{"version":1,"rules":[{{"pattern":"{pattern}","access":["read"]}}]}}"#),
        )?;

        let error = format!("{:#}", load_source(&root).unwrap_err());

        assert!(
            error.contains(&format!("{pattern:?}")),
            "{pattern}: {error}"
        );
        assert!(error.contains("**/"), "{pattern}: {error}");
        assert!(!error.contains("**/./"), "{pattern}: {error}");

        crate::shell::fixtures::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn unambiguous_patterns_are_accepted() -> Result<()> {
        let root = test_root("unambiguous_patterns_are_accepted")?;
        write_policy(
            &root,
            r#"{"version":1,"rules":[
  {"pattern":".env","access":["read"]},
  {"pattern":"*.pem","access":["read"]},
  {"pattern":"secrets/","access":["read"]},
  {"pattern":"**/secrets/**","access":["read"]},
  {"pattern":"~/.ssh/**","access":["read"]},
  {"pattern":"//etc/secret/**","access":["read"]},
  {"pattern":"/project/x","access":["write"]}
]}"#,
        )?;

        load_source(&root)?;

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
