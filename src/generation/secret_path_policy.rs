use std::path::Path;

use anyhow::{Context, Result, ensure};
use serde::Deserialize;

use crate::layout::SourceLayout;

#[derive(Debug, Deserialize)]
struct SecretPathPolicy {
    version: u64,
    rules: Vec<SecretPathRule>,
}

#[derive(Debug, Deserialize)]
struct SecretPathRule {
    pattern: String,
    access: Vec<SecretPathAccess>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SecretPathAccess {
    Read,
    Write,
}

impl SecretPathAccess {
    fn claude_permission(self, pattern: &str) -> String {
        match self {
            Self::Read => format!("Read({pattern})"),
            Self::Write => format!("Edit({pattern})"),
        }
    }
}

pub(crate) fn claude_deny_permissions(source: &Path) -> Result<Vec<String>> {
    let policy = read_policy(source)?;
    Ok(policy
        .rules
        .iter()
        .flat_map(|rule| {
            rule.access
                .iter()
                .map(|access| access.claude_permission(&rule.pattern))
        })
        .collect())
}

fn read_policy(source: &Path) -> Result<SecretPathPolicy> {
    let path = SourceLayout::new(source).secret_path_policy();
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let policy: SecretPathPolicy = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    validate_policy(&policy)?;
    Ok(policy)
}

fn validate_policy(policy: &SecretPathPolicy) -> Result<()> {
    ensure!(
        policy.version == 1,
        "unsupported secret path policy version: {}",
        policy.version
    );

    for rule in &policy.rules {
        ensure!(
            !rule.pattern.trim().is_empty(),
            "secret path policy patterns must not be empty"
        );
        ensure!(
            !rule.access.is_empty(),
            "secret path policy access list must not be empty"
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

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

        let permissions = claude_deny_permissions(&root)?;

        assert_eq!(
            permissions,
            vec![
                "Read(.env*)".to_string(),
                "Edit(.env*)".to_string(),
                "Read(~/.ssh/**)".to_string(),
            ],
        );

        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn invalid_policy_version_is_rejected() -> Result<()> {
        let root = test_root("invalid_policy_version_is_rejected")?;
        write_policy(&root, r#"{"version":2,"rules":[]}"#)?;

        let error = read_policy(&root).unwrap_err().to_string();

        assert!(error.contains("unsupported secret path policy version"));

        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    fn test_root(name: &str) -> Result<PathBuf> {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let root = std::env::temp_dir().join(format!("agent-harness-{name}-{nanos}"));
        std::fs::create_dir_all(&root)?;
        Ok(root)
    }

    fn write_policy(source: &Path, content: &str) -> Result<()> {
        let path = SourceLayout::new(source).secret_path_policy();
        path.parent().map(std::fs::create_dir_all).transpose()?;
        std::fs::write(path, content)?;
        Ok(())
    }
}
