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

/// The srt `denyRead` entries the policy requires, one per rule that denies reads.
///
/// As for Codex, a rule that only denies writes has no entry: srt's restrictions apply to every
/// process the agent starts, and the harness scripts write the paths such rules guard. Hooks keep
/// enforcing them.
///
/// srt reads every entry as a glob (`*`, `**`, `?`, `[...]`, no escapes), expands `~` and `~/`,
/// and resolves a relative entry against the working directory, on macOS and Linux alike; Pi's
/// extension re-anchors it at the Git toplevel.
///
/// # Errors
///
/// Returns an error naming the pattern when srt would read it differently from Claude.
pub(crate) fn srt_denied_read_paths(source: &Source) -> Result<Vec<String>> {
    source
        .secret_paths
        .rules
        .iter()
        .filter(|rule| rule.denies_read())
        .map(|rule| srt_path(rule.pattern.as_str()))
        .collect()
}

/// Resolves a Claude-style pattern to the entry srt reads the same way.
///
/// Claude reads `//x` as absolute and `/x` as relative to the project, and matches a single-segment
/// name (`.env`) at any depth. srt reads `/x` as absolute and a relative entry only beneath the
/// directory the harness anchors it at, so `/x` is rejected as ambiguous and a single-segment
/// name gets a `**/` prefix. A multi-segment relative pattern never reaches this function without
/// a leading `**/`: the policy rejects it on load. srt has no escapes, braces, negation, or
/// negated character classes, and on Linux ignores an absolute pattern whose first component has
/// a wildcard, so those are rejected too.
fn srt_path(pattern: &str) -> Result<String> {
    ensure!(
        !pattern.contains('\\'),
        "srt has no escape syntax, so {pattern:?} would match a name with a backslash; list each path as its own rule"
    );
    ensure!(
        !pattern.contains(['{', '}']),
        "srt does not expand braces, so {pattern:?} would match a literal name; list each path as its own rule"
    );
    ensure!(
        !pattern.contains("[!") && !pattern.contains("[^"),
        "srt does not negate character classes, so {pattern:?} would match a literal ! or ^; list the allowed characters instead"
    );
    let trimmed = pattern.trim_end_matches('/');
    let path =
        trimmed
            .strip_prefix("./")
            .unwrap_or(if trimmed.is_empty() { pattern } else { trimmed });
    ensure!(
        !path.starts_with('!'),
        "srt has no negation, so {path:?} cannot carve an exception out of another rule"
    );
    if let Some(absolute) = path.strip_prefix("//") {
        let root = absolute.split('/').next().unwrap_or_default();
        ensure!(
            !root.contains(['*', '?', '[', ']']),
            "srt on Linux skips an absolute pattern whose first component has a wildcard, so {path:?} would deny nothing; start it with a literal directory"
        );
        return Ok(format!("/{absolute}"));
    }
    ensure!(
        !path.starts_with('/'),
        "{path:?} is ambiguous for srt: write // for an absolute path or drop the leading / for a project path"
    );
    let is_home = path == "~" || path.starts_with("~/");
    let relative = path.strip_prefix("~/").unwrap_or(path);
    ensure!(
        is_home
            || !relative
                .split('/')
                .any(|part| matches!(part, "" | "." | "..")),
        "srt needs {path:?} to be absolute, start with ~/, or stay inside the project without . or .. parts"
    );
    if !is_home && is_single_segment_name(path) {
        return Ok(format!("**/{path}"));
    }
    Ok(path.to_owned())
}

fn is_single_segment_name(relative: &str) -> bool {
    !relative.contains('/') && !relative.starts_with("**")
}

/// A relative pattern with a `/` other than a trailing one is read at the root by gitignore and
/// Codex, at any depth by srt, and at the working directory by Claude. Only `**/` says "any depth".
fn ensure_unambiguous(pattern: &PathPattern) -> Result<()> {
    let text = pattern.as_str();
    let body = text.trim_end_matches('/');
    let suggestion = body.strip_prefix("./").unwrap_or(body);
    let is_anchored_elsewhere = text.starts_with('/') || text.starts_with("~/") || body == "~";
    ensure!(
        is_anchored_elsewhere || !body.contains('/') || body.starts_with("**/"),
        "secret path pattern {text:?} is ambiguous: gitignore and Codex anchor it at the repository root, srt matches it at any depth, and Claude anchors it at the working directory; write \"**/{suggestion}\" to match at any depth"
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
            "secret path policy access list must not be empty"
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
    fn srt_denied_read_paths_keep_only_rules_that_deny_reads() -> Result<()> {
        let root = test_root("srt_denied_read_paths_keep_only_rules_that_deny_reads")?;
        write_policy(
            &root,
            r#"{
  "version": 1,
  "rules": [
    {"pattern": ".env", "access": ["read", "write"]},
    {"pattern": "~/.ssh/**", "access": ["read"]},
    {"pattern": "**/secrets/**", "access": ["read"]},
    {"pattern": "//etc/secret/", "access": ["read"]},
    {"pattern": "**/src/keys/*", "access": ["read"]},
    {"pattern": "**/audit/**", "access": ["write"]}
  ]
}
"#,
        )?;

        let paths = srt_denied_read_paths(&load_source(&root)?)?;

        assert_eq!(
            paths,
            [
                "**/.env",
                "~/.ssh/**",
                "**/secrets/**",
                "/etc/secret",
                "**/src/keys/*"
            ],
        );

        crate::shell::fixtures::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn srt_rejects_patterns_it_would_read_differently() -> Result<()> {
        for pattern in [
            "a/{b,c}",
            "/project/x",
            "a/../b",
            "./",
            "a/key\\[1\\]",
            "!sample.env",
            "//**/.env",
            "//opt*/keys/**",
            "keys/[!a]*",
            "[^a-z].env",
        ] {
            let error = srt_path(pattern).unwrap_err().to_string();

            assert!(
                error.contains(&format!("{:?}", pattern.trim_end_matches('/'))),
                "{pattern}: {error}"
            );
        }
        Ok(())
    }

    #[test]
    fn ambiguous_relative_patterns_are_rejected_when_the_policy_is_loaded() -> Result<()> {
        for pattern in ["secrets/**", "config/x.json", "./secrets/**", "src/keys/*"] {
            let root = test_root("ambiguous_relative_patterns_are_rejected")?;
            write_policy(
                &root,
                &format!(
                    r#"{{"version":1,"rules":[{{"pattern":"{pattern}","access":["read"]}}]}}"#
                ),
            )?;

            let error = format!("{:#}", load_source(&root).unwrap_err());

            assert!(
                error.contains(&format!("{pattern:?}")),
                "{pattern}: {error}"
            );
            assert!(error.contains("**/"), "{pattern}: {error}");
            assert!(!error.contains("**/./"), "{pattern}: {error}");

            crate::shell::fixtures::remove_dir_all(root)?;
        }
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
