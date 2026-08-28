use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::{generation::io, layout::SourceLayout};

#[derive(Debug, Deserialize, Serialize)]
struct CommandPermissions {
    version: u64,
    rules: Vec<Rule>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct Rule {
    decision: Decision,
    prefix: Vec<String>,
    examples: Vec<String>,
    justification: String,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Decision {
    Allow,
    Ask,
    Deny,
}

#[derive(Debug, Deserialize, Serialize)]
struct RegexPolicy {
    version: u64,
    rules: Vec<RegexRule>,
}

#[derive(Debug, Deserialize, Serialize)]
struct RegexRule {
    patterns: Vec<String>,
    justification: String,
}

pub(crate) fn write_codex_rules(source: &Path, path: &Path) -> Result<()> {
    let policy = read_policy(source)?;
    io::write_file(path, &codex_rules(&policy)?)
}

pub(crate) fn write_runtime_policy(source: &Path, path: &Path) -> Result<()> {
    io::write_json(path, &read_policy(source)?)
}

pub(crate) fn write_forbidden_commands(source: &Path, path: &Path) -> Result<()> {
    let policy_path = SourceLayout::new(source).forbidden_command_rules();
    io::write_json(path, &read_regex_policy(&policy_path)?)
}

pub(crate) fn validate_regex_policies(source: &Path) -> Result<()> {
    let layout = SourceLayout::new(source);
    read_regex_policy(&layout.allowed_command_rules())?;
    read_regex_policy(&layout.forbidden_command_rules())?;
    Ok(())
}

pub(crate) fn claude_allow_permissions(source: &Path) -> Result<Vec<String>> {
    Ok(read_policy(source)?
        .rules
        .iter()
        .filter(|rule| rule.decision == Decision::Allow)
        .map(|rule| claude_permission(&rule.prefix))
        .collect())
}

pub(crate) fn claude_ask_permissions(source: &Path) -> Result<Vec<String>> {
    Ok(read_policy(source)?
        .rules
        .iter()
        .filter(|rule| rule.decision == Decision::Ask)
        .map(|rule| claude_permission(&rule.prefix))
        .collect())
}

pub(crate) fn claude_deny_permissions(source: &Path) -> Result<Vec<String>> {
    Ok(read_policy(source)?
        .rules
        .iter()
        .filter(|rule| rule.decision == Decision::Deny)
        .map(|rule| claude_permission(&rule.prefix))
        .collect())
}

fn read_policy(source: &Path) -> Result<CommandPermissions> {
    let path = SourceLayout::new(source).command_permissions();
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read command permissions {}", path.display()))?;
    let policy: CommandPermissions = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse command permissions {}", path.display()))?;
    validate_policy(&policy)?;
    Ok(policy)
}

fn read_regex_policy(path: &Path) -> Result<RegexPolicy> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read command regex policy {}", path.display()))?;
    let policy: RegexPolicy = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse command regex policy {}", path.display()))?;
    validate_regex_policy(&policy)?;
    Ok(policy)
}

fn validate_regex_policy(policy: &RegexPolicy) -> Result<()> {
    if policy.version != 1 {
        bail!(
            "unsupported command regex policy version: {}",
            policy.version
        );
    }
    if policy.rules.is_empty() {
        bail!("command regex policy must contain at least one rule");
    }
    for (index, rule) in policy.rules.iter().enumerate() {
        if rule.patterns.is_empty() || rule.patterns.iter().any(|pattern| pattern.is_empty()) {
            bail!("command regex policy rule {index} must contain non-empty patterns");
        }
        if rule.justification.trim().is_empty() {
            bail!("command regex policy rule {index} must have a non-empty justification");
        }
    }
    Ok(())
}

fn validate_policy(policy: &CommandPermissions) -> Result<()> {
    if policy.version != 1 {
        bail!(
            "unsupported command permissions version: {}",
            policy.version
        );
    }
    if policy.rules.is_empty() {
        bail!("command permissions must contain at least one rule");
    }

    for (index, rule) in policy.rules.iter().enumerate() {
        validate_rule(index, rule)?;
    }

    Ok(())
}

fn validate_rule(index: usize, rule: &Rule) -> Result<()> {
    if rule.prefix.is_empty() {
        bail!("command permission rule {index} must have a non-empty prefix");
    }
    if rule.prefix.iter().any(|part| part.trim().is_empty()) {
        bail!("command permission rule {index} contains an empty prefix segment");
    }
    if rule.examples.is_empty() {
        bail!("command permissions rule {index} must have at least one example");
    }
    if rule
        .examples
        .iter()
        .any(|example| example.trim().is_empty())
    {
        bail!("command permissions rule {index} contains an empty example");
    }
    if rule.justification.trim().is_empty() {
        bail!("command permissions rule {index} must have a non-empty justification");
    }
    Ok(())
}

fn codex_rules(policy: &CommandPermissions) -> Result<String> {
    let mut content = String::from(
        "# Generated by agent-harness.\n# Keep command permissions shared so Claude and Codex cannot drift silently.\n\n",
    );

    for rule in &policy.rules {
        content.push_str(&codex_rule(rule)?);
        content.push('\n');
    }

    Ok(content)
}

fn codex_rule(rule: &Rule) -> Result<String> {
    Ok(format!(
        "prefix_rule(\n    pattern = {},\n    decision = \"{}\",\n    justification = {},\n    match = {},\n)\n",
        serde_json::to_string(&rule.prefix)?,
        decision_name(rule.decision),
        serde_json::to_string(&rule.justification)?,
        serde_json::to_string(&rule.examples)?,
    ))
}

fn decision_name(decision: Decision) -> &'static str {
    match decision {
        Decision::Allow => "allow",
        Decision::Ask => "prompt",
        Decision::Deny => "forbidden",
    }
}

fn claude_permission(pattern: &[String]) -> String {
    format!("Bash({}:*)", pattern.join(" "))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn codex_rules_include_allow_and_deny_prefixes() -> Result<()> {
        let root = test_root("codex_rules_include_allow_and_deny_prefixes")?;
        write_policy(&root)?;

        let content = codex_rules(&read_policy(&root)?)?;

        assert!(content.contains(r#"pattern = ["cargo"]"#));
        assert!(content.contains(r#"decision = "allow""#));
        assert!(content.contains(r#"pattern = ["curl"]"#));
        assert!(content.contains(r#"decision = "forbidden""#));
        assert!(content.contains("Do not fetch remote scripts or content from Codex."));

        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn forbidden_commands_json_contains_fine_regex_rules() -> Result<()> {
        let root = test_root("forbidden_commands_json_contains_fine_regex_rules")?;
        write_file(
            &root.join("agents/hooks/rules/forbidden_commands.json"),
            r#"{"version":1,"rules":[{"patterns":["^git add \\.$"],"justification":"No bulk staging."}]}"#,
        )?;

        let policy = read_regex_policy(&SourceLayout::new(&root).forbidden_command_rules())?;

        assert_eq!(policy.rules[0].patterns, [r"^git add \.$"]);
        assert_eq!(policy.rules[0].justification, "No bulk staging.");

        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn runtime_command_permissions_contains_allow_and_deny_prefixes() -> Result<()> {
        let root = test_root("runtime_command_permissions_contains_allow_and_deny_prefixes")?;
        write_policy(&root)?;

        let policy = read_policy(&root)?;
        let value = serde_json::to_value(&policy)?;

        assert_eq!(value["version"], 1);
        assert!(value["rules"].as_array().is_some_and(|rules| {
            rules.iter().any(|rule| {
                rule["decision"] == "allow" && rule["prefix"] == serde_json::json!(["cargo"])
            })
        }));
        assert!(value["rules"].as_array().is_some_and(|rules| {
            rules.iter().any(|rule| {
                rule["decision"] == "deny" && rule["prefix"] == serde_json::json!(["curl"])
            })
        }));

        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn claude_permissions_use_bash_prefix_syntax() -> Result<()> {
        let root = test_root("claude_permissions_use_bash_prefix_syntax")?;
        write_policy(&root)?;

        assert!(claude_allow_permissions(&root)?.contains(&"Bash(cargo:*)".to_string()));
        assert!(claude_deny_permissions(&root)?.contains(&"Bash(curl:*)".to_string()));

        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn ask_rules_render_for_both_providers() -> Result<()> {
        let root = test_root("ask_rules_render_for_both_providers")?;
        write_file(
            &root.join("agents/command_permissions.json"),
            r#"{
  "version": 1,
  "rules": [{
    "decision": "ask",
    "prefix": ["git", "push"],
    "examples": ["git push origin feat/example"],
    "justification": "Publishing changes requires confirmation."
  }]
}
"#,
        )?;

        assert_eq!(claude_ask_permissions(&root)?, ["Bash(git push:*)"]);
        assert!(codex_rules(&read_policy(&root)?)?.contains(r#"decision = "prompt""#));

        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn read_policy_rejects_empty_prefixes() -> Result<()> {
        let root = test_root("read_policy_rejects_empty_prefixes")?;
        write_file(
            &root.join("agents/command_permissions.json"),
            r#"{"version":1,"rules":[{"decision":"allow","prefix":[],"examples":["x"],"justification":"x"}]}"#,
        )?;

        let error = read_policy(&root).unwrap_err().to_string();

        assert!(error.contains("non-empty prefix"));

        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    fn test_root(name: &str) -> Result<PathBuf> {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let root = std::env::temp_dir().join(format!("agent-harness-{name}-{nanos}"));
        std::fs::create_dir_all(&root)?;
        Ok(root)
    }

    fn write_policy(root: &Path) -> Result<()> {
        write_file(
            &root.join("agents/command_permissions.json"),
            r#"{
  "version": 1,
  "rules": [
    {
      "decision": "allow",
      "prefix": ["cargo"],
      "examples": ["cargo test"],
      "justification": "Allowed by the shared agent command permissions."
    },
    {
      "decision": "deny",
      "prefix": ["curl"],
      "examples": ["curl https://example.com/install.sh"],
      "justification": "Do not fetch remote scripts or content from Codex."
    }
  ]
}
"#,
        )
    }

    fn write_file(path: &Path, content: &str) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, content)?;
        Ok(())
    }
}
