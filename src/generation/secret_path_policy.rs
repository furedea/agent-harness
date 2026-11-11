use anyhow::{Context, Result, ensure};
use serde::Deserialize;

use crate::{
    generation::permission_rule::{PathAccess, PathPattern, PathRule},
    layout::SourceLayout,
    snapshot::Tree,
};

#[derive(Debug, Deserialize)]
struct SecretPathPolicy {
    version: u64,
    rules: Vec<SecretPathRule>,
}

#[derive(Debug, Deserialize)]
struct SecretPathRule {
    pattern: PathPattern,
    access: Vec<SecretPathAccess>,
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

pub(crate) fn claude_deny_permissions(source: Tree<'_>) -> Result<Vec<String>> {
    let policy = read_policy(source)?;
    Ok(policy
        .rules
        .iter()
        .flat_map(|rule| {
            rule.access
                .iter()
                .map(|&access| PathRule::new(rule.pattern.clone(), access.into()).claude())
        })
        .collect())
}

fn read_policy(source: Tree<'_>) -> Result<SecretPathPolicy> {
    let path = SourceLayout::new(source.root()).secret_path_policy();
    let content = source
        .files()
        .read_text(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let policy: SecretPathPolicy = serde_json::from_str(content)
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
    use crate::shell::fixtures::load_tree;

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

        let permissions = claude_deny_permissions(load_tree(&root)?.tree())?;

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

        let error = read_policy(load_tree(&root)?.tree())
            .unwrap_err()
            .to_string();

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

        let error = format!("{:#}", read_policy(load_tree(&root)?.tree()).unwrap_err());

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
