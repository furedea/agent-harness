use std::collections::HashSet;

use anyhow::{Result, bail};
use serde_json::{Map, Value};

/// Claude Code permission rule lists collected from the source settings and
/// the generated entries, ordered from least to most restrictive.
#[derive(Debug)]
pub(crate) struct PermissionRules {
    allow: Vec<String>,
    ask: Vec<String>,
    deny: Vec<String>,
}

impl PermissionRules {
    /// Reads the `allow`, `ask`, and `deny` lists declared by a `permissions`
    /// object. Missing lists are empty; non-array lists or non-string entries
    /// are rejected.
    pub(crate) fn parse(permissions: &Map<String, Value>) -> Result<Self> {
        Ok(Self {
            allow: rule_list(permissions, "allow")?,
            ask: rule_list(permissions, "ask")?,
            deny: rule_list(permissions, "deny")?,
        })
    }

    pub(crate) fn extend_allow(&mut self, rules: impl IntoIterator<Item = String>) {
        self.allow.extend(rules);
    }

    pub(crate) fn extend_ask(&mut self, rules: impl IntoIterator<Item = String>) {
        self.ask.extend(rules);
    }

    pub(crate) fn extend_deny(&mut self, rules: impl IntoIterator<Item = String>) {
        self.deny.extend(rules);
    }

    /// Replaces the `allow`, `ask`, and `deny` lists of `permissions` with the
    /// collected rules in insertion order. Every rule appears once, and a rule
    /// declared in more than one list is kept only in its most restrictive
    /// list so merging never widens a permission. Other keys are untouched.
    pub(crate) fn write_into(self, permissions: &mut Map<String, Value>) {
        let deny = unique_rules(self.deny, &[]);
        let ask = unique_rules(self.ask, &[&deny]);
        let allow = unique_rules(self.allow, &[&deny, &ask]);

        permissions.insert("allow".to_owned(), rule_array(allow));
        permissions.insert("ask".to_owned(), rule_array(ask));
        permissions.insert("deny".to_owned(), rule_array(deny));
    }
}

fn rule_list(permissions: &Map<String, Value>, key: &str) -> Result<Vec<String>> {
    let Some(value) = permissions.get(key) else {
        return Ok(Vec::new());
    };
    let Some(entries) = value.as_array() else {
        bail!("permissions.{key} must be a JSON array");
    };

    entries
        .iter()
        .map(|entry| match entry.as_str() {
            Some(rule) => Ok(rule.to_owned()),
            None => bail!("permissions.{key} entries must be strings, found {entry}"),
        })
        .collect()
}

fn unique_rules(rules: Vec<String>, stricter_lists: &[&[String]]) -> Vec<String> {
    let mut seen: HashSet<&str> = stricter_lists
        .iter()
        .flat_map(|list| list.iter().map(String::as_str))
        .collect();
    let mut unique = Vec::new();
    for rule in &rules {
        if seen.insert(rule.as_str()) {
            unique.push(rule.clone());
        }
    }
    unique
}

fn rule_array(rules: Vec<String>) -> Value {
    Value::Array(rules.into_iter().map(Value::String).collect())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn write_into_keeps_source_rules_before_generated_rules() -> Result<()> {
        let mut permissions = permissions_object(json!({
            "allow": ["Read"],
            "ask": ["Bash(dangerouslyDisableSandbox:true)"],
            "defaultMode": "acceptEdits"
        }));
        let mut rules = PermissionRules::parse(&permissions)?;
        rules.extend_allow(["Bash(cargo:*)".to_owned()]);
        rules.extend_ask(["Bash(git push:*)".to_owned()]);
        rules.extend_deny(["Bash(curl:*)".to_owned()]);

        rules.write_into(&mut permissions);

        assert_eq!(
            Value::Object(permissions),
            json!({
                "allow": ["Read", "Bash(cargo:*)"],
                "ask": ["Bash(dangerouslyDisableSandbox:true)", "Bash(git push:*)"],
                "deny": ["Bash(curl:*)"],
                "defaultMode": "acceptEdits"
            }),
        );
        Ok(())
    }

    #[test]
    fn parse_rejects_non_string_rules() {
        let permissions = permissions_object(json!({"allow": ["Read", 1]}));

        let error = PermissionRules::parse(&permissions)
            .unwrap_err()
            .to_string();

        assert!(error.contains("permissions.allow entries must be strings"));
    }

    #[test]
    fn parse_rejects_a_non_array_list() {
        let permissions = permissions_object(json!({"ask": "Read"}));

        let error = PermissionRules::parse(&permissions)
            .unwrap_err()
            .to_string();

        assert!(error.contains("permissions.ask must be a JSON array"));
    }

    #[test]
    fn write_into_lists_each_rule_once() -> Result<()> {
        let mut permissions = permissions_object(json!({
            "allow": ["Read", "Bash(cargo:*)", "Read"]
        }));
        let mut rules = PermissionRules::parse(&permissions)?;
        rules.extend_allow(["Bash(cargo:*)".to_owned()]);

        rules.write_into(&mut permissions);

        assert_eq!(permissions["allow"], json!(["Read", "Bash(cargo:*)"]));
        Ok(())
    }

    #[test]
    fn write_into_keeps_a_rule_only_in_its_most_restrictive_list() -> Result<()> {
        let mut permissions = permissions_object(json!({
            "allow": ["Bash(curl:*)", "Bash(git push:*)", "Read"],
            "ask": ["Bash(git push:*)"]
        }));
        let mut rules = PermissionRules::parse(&permissions)?;
        rules.extend_ask(["Bash(curl:*)".to_owned()]);
        rules.extend_deny(["Bash(curl:*)".to_owned()]);

        rules.write_into(&mut permissions);

        assert_eq!(permissions["allow"], json!(["Read"]));
        assert_eq!(permissions["ask"], json!(["Bash(git push:*)"]));
        assert_eq!(permissions["deny"], json!(["Bash(curl:*)"]));
        Ok(())
    }

    fn permissions_object(value: Value) -> Map<String, Value> {
        match value {
            Value::Object(object) => object,
            _ => panic!("permissions fixture must be a JSON object"),
        }
    }
}
