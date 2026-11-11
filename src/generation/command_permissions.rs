use std::path::Path;

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};

use crate::{
    file_op::FileOp,
    generation::permission_rule::PrefixRule,
    layout::SourceLayout,
    snapshot::{Snapshot, Tree},
};

#[derive(Debug, Deserialize, Serialize)]
struct CommandPermissions {
    version: u64,
    rules: Vec<Rule>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct Rule {
    decision: Decision,
    prefix: PrefixRule,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    programs: Option<Vec<String>>,
    patterns: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    exceptions: Option<Vec<String>>,
    justification: String,
}

pub(crate) fn codex_rules_op(source: Tree<'_>, path: &Path) -> Result<FileOp> {
    let policy = read_policy(source)?;
    Ok(FileOp::write(path, codex_rules(&policy)?))
}

pub(crate) fn runtime_policy_op(source: Tree<'_>, path: &Path) -> Result<FileOp> {
    FileOp::write_json(path, &read_policy(source)?)
}

pub(crate) fn forbidden_commands_op(source: Tree<'_>, path: &Path) -> Result<FileOp> {
    let policy_path = SourceLayout::new(source.root()).forbidden_command_rules();
    FileOp::write_json(path, &read_regex_policy(source.files(), &policy_path)?)
}

pub(crate) fn validate_regex_policies(source: Tree<'_>) -> Result<()> {
    let layout = SourceLayout::new(source.root());
    read_regex_policy(source.files(), &layout.allowed_command_rules())?;
    read_regex_policy(source.files(), &layout.forbidden_command_rules())?;
    Ok(())
}

pub(crate) fn claude_allow_permissions(source: Tree<'_>) -> Result<Vec<String>> {
    Ok(read_policy(source)?
        .rules
        .iter()
        .filter(|rule| rule.decision == Decision::Allow)
        .map(|rule| rule.prefix.claude())
        .collect())
}

pub(crate) fn claude_ask_permissions(source: Tree<'_>) -> Result<Vec<String>> {
    Ok(read_policy(source)?
        .rules
        .iter()
        .filter(|rule| rule.decision == Decision::Ask)
        .map(|rule| rule.prefix.claude())
        .collect())
}

pub(crate) fn claude_deny_permissions(source: Tree<'_>) -> Result<Vec<String>> {
    Ok(read_policy(source)?
        .rules
        .iter()
        .filter(|rule| rule.decision == Decision::Deny)
        .map(|rule| rule.prefix.claude())
        .collect())
}

pub(crate) fn devin_permissions(source: Tree<'_>) -> Result<serde_json::Value> {
    let policy = read_policy(source)?;
    Ok(serde_json::json!({
        "allow": devin_permissions_for(&policy, Decision::Allow),
        "ask": devin_permissions_for(&policy, Decision::Ask),
        "deny": devin_permissions_for(&policy, Decision::Deny),
    }))
}

fn devin_permissions_for(policy: &CommandPermissions, decision: Decision) -> Vec<String> {
    policy
        .rules
        .iter()
        .filter(|rule| rule.decision == decision)
        .map(|rule| rule.prefix.devin())
        .collect()
}

fn read_policy(source: Tree<'_>) -> Result<CommandPermissions> {
    let path = SourceLayout::new(source.root()).command_permissions();
    let content = source
        .files()
        .read_text(&path)
        .with_context(|| format!("failed to read command permissions {}", path.display()))?;
    let policy: CommandPermissions = serde_json::from_str(content)
        .with_context(|| format!("failed to parse command permissions {}", path.display()))?;
    validate_policy(&policy)?;
    Ok(policy)
}

fn read_regex_policy(files: &Snapshot, path: &Path) -> Result<RegexPolicy> {
    let content = files
        .read_text(path)
        .with_context(|| format!("failed to read command regex policy {}", path.display()))?;
    let policy: RegexPolicy = serde_json::from_str(content)
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
    for (index, rule) in policy.rules.iter().enumerate() {
        ensure!(
            !rule.patterns.is_empty() && !rule.patterns.iter().any(String::is_empty),
            "command regex policy rule {index} must contain non-empty patterns"
        );
        ensure!(
            !rule.justification.trim().is_empty(),
            "command regex policy rule {index} must have a non-empty justification"
        );
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
    for (index, rule) in policy.rules.iter().enumerate() {
        validate_rule(index, rule)?;
    }

    Ok(())
}

fn validate_rule(index: usize, rule: &Rule) -> Result<()> {
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
        "prefix_rule(\n    pattern = {},\n    decision = \"{}\",\n    justification = {},\n)\n",
        serde_json::to_string(rule.prefix.tokens())?,
        decision_name(rule.decision),
        serde_json::to_string(&rule.justification)?,
    ))
}

fn decision_name(decision: Decision) -> &'static str {
    match decision {
        Decision::Allow => "allow",
        Decision::Ask => "prompt",
        Decision::Deny => "forbidden",
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use crate::shell::fixtures::load_tree;

    fn write_forbidden_commands(source: &Path, path: &Path) -> Result<()> {
        let files = load_tree(source)?;
        crate::shell::execute(&[forbidden_commands_op(files.tree(), path)?])
    }

    #[test]
    fn codex_rules_include_allow_and_deny_prefixes() -> Result<()> {
        let root = test_root("codex_rules_include_allow_and_deny_prefixes")?;
        write_policy(&root)?;

        let content = codex_rules(&read_policy(load_tree(&root)?.tree())?)?;

        assert!(content.contains(r#"pattern = ["cargo"]"#));
        assert!(content.contains(r#"decision = "allow""#));
        assert!(content.contains(r#"pattern = ["curl"]"#));
        assert!(content.contains(r#"decision = "forbidden""#));
        assert!(content.contains("Do not fetch remote scripts or content from Codex."));

        crate::shell::fixtures::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn forbidden_commands_json_contains_fine_regex_rules() -> Result<()> {
        let root = test_root("forbidden_commands_json_contains_fine_regex_rules")?;
        write_file(
            &root.join("hooks/rules/forbidden_commands.json"),
            r#"{"version":1,"rules":[{"patterns":["^git add \\.$"],"justification":"No bulk staging."}]}"#,
        )?;

        let policy = read_regex_policy(
            load_tree(&root)?.files(),
            &SourceLayout::new(&root).forbidden_command_rules(),
        )?;

        assert_eq!(policy.rules[0].patterns, [r"^git add \.$"]);
        assert_eq!(policy.rules[0].justification, "No bulk staging.");

        crate::shell::fixtures::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn generated_forbidden_commands_keep_every_source_rule_field() -> Result<()> {
        let root = test_root("generated_forbidden_commands_keep_every_source_rule_field")?;
        let source = serde_json::json!({
            "version": 1,
            "rules": [
                {
                    "patterns": ["^git add \\."],
                    "justification": "No bulk staging."
                },
                {
                    "patterns": ["^herdr agent start .*--"],
                    "exceptions": ["^herdr agent start [a-z]+ -- --model sonnet$"],
                    "justification": "Restrict agent arguments."
                },
                {
                    "programs": ["cargo", "uv"],
                    "patterns": ["^uv run .*mutmut apply"],
                    "justification": "Do not write mutants into the working tree."
                },
            ],
        });
        write_file(
            &root.join("hooks/rules/forbidden_commands.json"),
            &serde_json::to_string(&source)?,
        )?;
        let generated_path = root.join("generated.json");

        write_forbidden_commands(&root, &generated_path)?;

        let generated: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&generated_path)?)?;
        assert_eq!(generated, source);

        crate::shell::fixtures::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn runtime_command_permissions_contains_allow_and_deny_prefixes() -> Result<()> {
        let root = test_root("runtime_command_permissions_contains_allow_and_deny_prefixes")?;
        write_policy(&root)?;

        let policy = read_policy(load_tree(&root)?.tree())?;
        let value = serde_json::to_value(&policy)?;

        assert_eq!(value["version"], 1);
        assert!(has_rule(&value, "allow", &serde_json::json!(["cargo"])));
        assert!(has_rule(&value, "deny", &serde_json::json!(["curl"])));

        crate::shell::fixtures::remove_dir_all(root)?;
        Ok(())
    }

    fn has_rule(policy: &serde_json::Value, decision: &str, prefix: &serde_json::Value) -> bool {
        let matches =
            |rule: &serde_json::Value| rule["decision"] == decision && rule["prefix"] == *prefix;
        policy["rules"]
            .as_array()
            .is_some_and(|rules| rules.iter().any(matches))
    }

    #[test]
    fn claude_permissions_use_bash_prefix_syntax() -> Result<()> {
        let root = test_root("claude_permissions_use_bash_prefix_syntax")?;
        write_policy(&root)?;

        assert!(
            claude_allow_permissions(load_tree(&root)?.tree())?
                .contains(&"Bash(cargo:*)".to_string())
        );
        assert!(
            claude_deny_permissions(load_tree(&root)?.tree())?
                .contains(&"Bash(curl:*)".to_string())
        );

        crate::shell::fixtures::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn ask_rules_render_as_claude_ask_permissions() -> Result<()> {
        let root = test_root("ask_rules_render_as_claude_ask_permissions")?;
        write_file(
            &root.join("command_permissions.json"),
            r#"{
  "version": 1,
  "rules": [{
    "decision": "ask",
    "prefix": ["git", "push"],
    "justification": "Publishing changes requires confirmation."
  }]
}
"#,
        )?;

        let permissions = claude_ask_permissions(load_tree(&root)?.tree())?;

        assert_eq!(permissions, ["Bash(git push:*)"]);

        crate::shell::fixtures::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn devin_permissions_use_exec_prefix_syntax() -> Result<()> {
        let root = test_root("devin_permissions_use_exec_prefix_syntax")?;
        write_policy(&root)?;

        let permissions = devin_permissions(load_tree(&root)?.tree())?;

        assert_eq!(permissions["allow"], serde_json::json!(["Exec(cargo)"]));
        assert_eq!(permissions["deny"], serde_json::json!(["Exec(curl)"]));

        crate::shell::fixtures::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn devin_permissions_join_prefix_segments_with_spaces() -> Result<()> {
        let root = test_root("devin_permissions_join_prefix_segments_with_spaces")?;
        write_file(
            &root.join("command_permissions.json"),
            r#"{
  "version": 1,
  "rules": [{
    "decision": "ask",
    "prefix": ["git", "push"],
    "justification": "Publishing changes requires confirmation."
  }]
}
"#,
        )?;

        let permissions = devin_permissions(load_tree(&root)?.tree())?;

        assert_eq!(permissions["ask"], serde_json::json!(["Exec(git push)"]));

        crate::shell::fixtures::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn ask_rules_render_as_codex_prompt_decisions() -> Result<()> {
        let root = test_root("ask_rules_render_as_codex_prompt_decisions")?;
        write_file(
            &root.join("command_permissions.json"),
            r#"{
  "version": 1,
  "rules": [{
    "decision": "ask",
    "prefix": ["git", "push"],
    "justification": "Publishing changes requires confirmation."
  }]
}
"#,
        )?;

        let rules = codex_rules(&read_policy(load_tree(&root)?.tree())?)?;

        assert!(rules.contains(r#"decision = "prompt""#));
        assert!(!rules.contains("match ="));

        crate::shell::fixtures::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn read_policy_rejects_empty_prefixes() -> Result<()> {
        let root = test_root("read_policy_rejects_empty_prefixes")?;
        write_file(
            &root.join("command_permissions.json"),
            r#"{"version":1,"rules":[{"decision":"allow","prefix":[],"justification":"x"}]}"#,
        )?;

        let error = format!("{:#}", read_policy(load_tree(&root)?.tree()).unwrap_err());

        assert!(error.contains("non-empty"));

        crate::shell::fixtures::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn read_policy_rejects_whitespace_inside_a_prefix_segment() -> Result<()> {
        let root = test_root("read_policy_rejects_whitespace_inside_a_prefix_segment")?;
        write_file(
            &root.join("command_permissions.json"),
            r#"{"version":1,"rules":[{"decision":"allow","prefix":["rm -rf"],"justification":"x"}]}"#,
        )?;

        let error = format!("{:#}", read_policy(load_tree(&root)?.tree()).unwrap_err());

        assert!(error.contains("whitespace"));

        crate::shell::fixtures::remove_dir_all(root)?;
        Ok(())
    }

    fn test_root(name: &str) -> Result<PathBuf> {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let root = std::env::temp_dir().join(format!("agent-harness-{name}-{nanos}"));
        crate::shell::fixtures::create_dir_all(&root)?;
        Ok(root)
    }

    fn write_policy(root: &Path) -> Result<()> {
        write_file(
            &root.join("command_permissions.json"),
            r#"{
  "version": 1,
  "rules": [
    {
      "decision": "allow",
      "prefix": ["cargo"],
      "justification": "Allowed by the shared agent command permissions."
    },
    {
      "decision": "deny",
      "prefix": ["curl"],
      "justification": "Do not fetch remote scripts or content from Codex."
    }
  ]
}
"#,
        )
    }

    fn write_file(path: &Path, content: &str) -> Result<()> {
        path.parent()
            .map(crate::shell::fixtures::create_dir_all)
            .transpose()?;
        crate::shell::fixtures::write(path, content)?;
        Ok(())
    }
}
