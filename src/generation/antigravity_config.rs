#![forbid(clippy::disallowed_methods, clippy::disallowed_types)]

use anyhow::{Context, Result, bail};
use serde_json::{Map, Value};

use crate::{
    config_sync::codec::Json,
    generation::permission_rule::PathAccess,
    generation::{
        GenerationContext, claude_permissions::PermissionRules, command_permissions, protection,
        secret_path_policy,
    },
    provider::spec::ProviderId,
};

/// Directories `agy --sandbox` must read for `git` and `nix` to run; `agy` rejects globs.
const SANDBOX_READ_PATHS: [&str; 2] = ["/nix", "/run"];

/// Well-known secret mounts under `/run` (sops-nix and agenix link their names to generation
/// directories, and systemd keeps service credentials in `/run/credentials`), which the allow above would otherwise cover.
const RUN_SECRET_PATHS: [&str; 8] = [
    "/run/credentials",
    "/run/secrets",
    "/run/secrets.d",
    "/run/secrets-for-users",
    "/run/agenix",
    "/run/agenix.d",
    "/run/keys",
    "/run/user",
];

/// The Antigravity settings: the source settings with the generated parts merged in.
///
/// `agy` reads `permissions` rules for six resource kinds, takes absolute paths only, and fails
/// to start the sandbox on a glob, so every path rule here is a concrete file or directory.
/// Patterns that need a glob stay enforced by the `PreToolUse` hook.
pub(crate) fn settings_for_runtime(context: GenerationContext<'_>, source: &Json) -> Result<Json> {
    let mut settings = source.clone().into_value();
    let root = settings
        .as_object_mut()
        .context("Antigravity settings root must be a JSON object")?;
    // An empty `agentMode` is Default mode, which keeps Auto mode from skipping the rules.
    root.insert("agentMode".to_owned(), Value::String(String::new()));
    root.insert("enableTerminalSandbox".to_owned(), Value::Bool(true));
    merge_permissions(root, context)?;
    Json::from_value(settings)
}

fn merge_permissions(root: &mut Map<String, Value>, context: GenerationContext<'_>) -> Result<()> {
    let source = context.source;
    let permissions = root
        .entry("permissions".to_owned())
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .context("permissions must be a JSON object")?;
    reject_unusable_path_rules(permissions)?;
    let mut rules = PermissionRules::parse(permissions)?;

    rules.extend_allow(SANDBOX_READ_PATHS.map(read_file_rule));
    rules.extend_allow(command_permissions::antigravity_allow_permissions(source));
    rules.extend_ask(command_permissions::antigravity_ask_permissions(source));
    rules.extend_deny(command_permissions::antigravity_deny_permissions(source));
    rules.extend_deny(RUN_SECRET_PATHS.map(read_file_rule));
    rules.extend_deny(
        secret_path_policy::antigravity_denied_paths(source, PathAccess::Read)
            .into_iter()
            .map(|path| read_file_rule(&path)),
    );
    rules.extend_deny(
        secret_path_policy::antigravity_denied_paths(source, PathAccess::Write)
            .into_iter()
            .map(|path| write_file_rule(&path)),
    );
    rules.extend_deny(
        protected_write_paths(context)?
            .iter()
            .map(|path| write_file_rule(path)),
    );
    rules.write_into(permissions);
    Ok(())
}

/// The installed harness files, plus the whole Antigravity config directory and its settings
/// file: the hook registration and the settings that carry these rules must not be editable.
/// Fails on a source `read_file(...)` or `write_file(...)` rule `agy` cannot start with: it takes
/// absolute paths, and a glob makes the sandbox fail to start.
fn reject_unusable_path_rules(permissions: &Map<String, Value>) -> Result<()> {
    let mut rules = ["allow", "ask", "deny"]
        .iter()
        .filter_map(|list| permissions.get(*list)?.as_array())
        .flatten()
        .filter_map(Value::as_str);
    match rules.find(|rule| is_unusable_path_rule(rule)) {
        Some(rule) => bail!(
            "the Antigravity rule {rule:?} must be `tool(path)` with an absolute path or one starting with ~/ \
             and no glob; agy fails to start its sandbox on a glob"
        ),
        None => Ok(()),
    }
}

fn is_unusable_path_rule(rule: &str) -> bool {
    let Some(wrapped) = rule
        .strip_prefix("read_file(")
        .or_else(|| rule.strip_prefix("write_file("))
    else {
        return false;
    };
    let Some(path) = wrapped.strip_suffix(')') else {
        return true;
    };
    let is_anchored = path.starts_with(['/', '~']);
    path.contains(['*', '?', '[', ']', '{', '}', '(', ')']) || !is_anchored
}

fn protected_write_paths(context: GenerationContext<'_>) -> Result<Vec<String>> {
    let spec = ProviderId::Antigravity.spec();
    let Some(settings) = spec.config_file() else {
        bail!("the {} provider declares no settings file", spec.key);
    };
    let root = context.runtime_root;
    let mut paths = vec![
        root.path(std::path::Path::new(spec.home_dir)),
        root.path(std::path::Path::new(settings)),
    ];
    paths.extend(protection::protected_paths_for_runtime(context)?);
    Ok(unique(paths))
}

fn unique(paths: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    paths
        .into_iter()
        .filter(|path| seen.insert(path.clone()))
        .collect()
}

pub(crate) fn read_file_rule(path: &str) -> String {
    format!("read_file({path})")
}

fn write_file_rule(path: &str) -> String {
    format!("write_file({path})")
}

#[cfg(test)]
mod tests;
