use anyhow::Result;

use super::{CommandPermissions, Decision};
use crate::source_model::Source;

/// The allow rules `agy` can express: a prefix it would read as a pattern is left out, since a
/// missing allow only narrows what runs, and a provider that is switched off must not fail the build.
pub(crate) fn antigravity_allow_permissions(source: &Source) -> Vec<String> {
    let allowed = |decision| decision == Decision::Allow;
    let rules = source.command_permissions.rules.iter();
    rules
        .filter(|rule| allowed(rule.decision))
        .filter_map(|rule| rule.prefix.antigravity().ok())
        .collect()
}

pub(crate) fn antigravity_ask_permissions(source: &Source) -> Result<Vec<String>> {
    antigravity_rules(&source.command_permissions, Decision::Ask)
}

pub(crate) fn antigravity_deny_permissions(source: &Source) -> Result<Vec<String>> {
    antigravity_rules(&source.command_permissions, Decision::Deny)
}

fn antigravity_rules(policy: &CommandPermissions, decision: Decision) -> Result<Vec<String>> {
    let rules = policy.rules.iter();
    rules
        .filter(|rule| rule.decision == decision)
        .map(|rule| rule.prefix.antigravity())
        .collect()
}
