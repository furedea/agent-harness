use super::{CommandPermissions, Decision};
use crate::source_model::Source;

/// The `command(...)` rules of one decision that `agy` can express.
///
/// A prefix it would read as a pattern (`regex:`, a lone `*`, parentheses) is left out of every
/// decision: the shared policy is valid for the other providers, so it must not fail the build, and
/// the command guard hook that every provider runs still enforces it.
pub(crate) fn antigravity_allow_permissions(source: &Source) -> Vec<String> {
    antigravity_rules(&source.command_permissions, Decision::Allow)
}

pub(crate) fn antigravity_ask_permissions(source: &Source) -> Vec<String> {
    antigravity_rules(&source.command_permissions, Decision::Ask)
}

pub(crate) fn antigravity_deny_permissions(source: &Source) -> Vec<String> {
    antigravity_rules(&source.command_permissions, Decision::Deny)
}

fn antigravity_rules(policy: &CommandPermissions, decision: Decision) -> Vec<String> {
    let rules = policy.rules.iter();
    rules
        .filter(|rule| rule.decision == decision)
        .filter_map(|rule| rule.prefix.antigravity().ok())
        .collect()
}
