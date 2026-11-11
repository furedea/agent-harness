use std::path::Path;

use anyhow::{Context, Result};
use toml_edit::{DocumentMut, Item, TableLike};

use crate::{
    fs_ops,
    generation::{
        codex_managed_state::{
            ManagedState, managed_state_path, read_managed_state, write_managed_state,
        },
        external_hooks::ExternalHookBundle,
        io, protection,
    },
    layout::SourceLayout,
    runtime_root::RuntimeRoot,
};

const MANAGED_KEYS: &[&str] = &[
    "model",
    "model_reasoning_effort",
    "personality",
    "approval_policy",
    "sandbox_mode",
    "approvals_reviewer",
    "cli_auth_credentials_store",
    "notice",
    "tui",
    "plugins",
    "features",
    "default_permissions",
    "permissions",
];

pub(crate) fn write_config_source(
    source: &Path,
    out: &Path,
    external_hooks: &[ExternalHookBundle],
) -> Result<()> {
    io::write_file(out, &config_source_content(source, external_hooks)?)
}

pub(crate) fn sync_managed_config(source_path: &Path, target_path: &Path) -> Result<()> {
    let source = read_toml_document(source_path)?;
    sync_managed_document(&source, target_path)
}

pub(crate) fn generated_config_source(
    context: crate::generation::GenerationContext<'_>,
) -> Result<DocumentMut> {
    config_source_content_for_runtime(context)?
        .parse::<DocumentMut>()
        .context("failed to parse generated Codex config source")
}

fn config_source_content(source: &Path, external_hooks: &[ExternalHookBundle]) -> Result<String> {
    config_source_content_for_runtime(crate::generation::GenerationContext {
        source,
        external_hooks,
        runtime_root: &RuntimeRoot::home(),
        dsh_profile: "web",
    })
}

fn config_source_content_for_runtime(
    context: crate::generation::GenerationContext<'_>,
) -> Result<String> {
    let crate::generation::GenerationContext {
        source,
        external_hooks,
        ..
    } = context;
    let base_path = SourceLayout::new(source).codex_config();
    let base = std::fs::read_to_string(&base_path)
        .with_context(|| format!("failed to read TOML file {}", base_path.display()))?;
    let mut document = base
        .parse::<DocumentMut>()
        .with_context(|| format!("failed to parse TOML file {}", base_path.display()))?;
    // Merge rather than concatenate: the source may already declare guarded entries, such as
    // the agent GitHub token's read denials.
    let protection = protection::codex_config_fragment_for_runtime(context)?
        .parse::<DocumentMut>()
        .context("failed to parse generated Codex config")?;
    for (key, item) in protection.iter() {
        merge_item(document.entry(key).or_insert(Item::None), item);
    }
    for bundle in external_hooks {
        merge_external_config(&mut document, bundle)?;
    }
    Ok(document.to_string())
}

fn merge_external_config(document: &mut DocumentMut, bundle: &ExternalHookBundle) -> Result<()> {
    let Some(path) = bundle.codex_config_path()? else {
        return Ok(());
    };
    let generated = read_toml_document(&path)?;
    merge_external_features(document, &generated, &path)
}

fn merge_external_features(
    target: &mut DocumentMut,
    source: &DocumentMut,
    path: &Path,
) -> Result<()> {
    for (key, item) in source.iter() {
        anyhow::ensure!(
            key == "features",
            "external hook config {} contains unsupported top-level key: {key}",
            path.display(),
        );
        merge_item(&mut target[key], item);
    }
    Ok(())
}

fn merge_item(target: &mut Item, source: &Item) {
    match (target.as_table_like_mut(), source.as_table_like()) {
        (Some(target), Some(source)) => merge_tables(target, source),
        _ => *target = source.clone(),
    }
}

fn merge_tables(target: &mut dyn TableLike, source: &dyn TableLike) {
    for (key, item) in source.iter() {
        merge_item(target.entry(key).or_insert(Item::None), item);
    }
}

pub(crate) fn sync_managed_document(source: &DocumentMut, target_path: &Path) -> Result<()> {
    let mut target = if target_path.exists() {
        read_toml_document(target_path)?
    } else {
        DocumentMut::new()
    };

    sync_managed_keys(source, &mut target);
    let state_path = managed_state_path(target_path);
    let state = sync_env_keys(source, &mut target, &read_managed_state(&state_path))?;

    write_toml_document(target_path, &target)?;
    write_managed_state(&state_path, &state)
}

fn sync_managed_keys(source: &DocumentMut, target: &mut DocumentMut) {
    for key in MANAGED_KEYS {
        sync_key(source, target, key);
    }
}

fn sync_key(source: &DocumentMut, target: &mut DocumentMut, key: &str) {
    match source.get(key) {
        Some(item) => target[key] = item.clone(),
        None => {
            target.remove(key);
        }
    }
}

/// Writes every generated `shell_environment_policy.set` key and removes the ones the previous
/// sync wrote that the source no longer has. Returns the keys now managed.
fn sync_env_keys(
    source: &DocumentMut,
    target: &mut DocumentMut,
    previous: &ManagedState,
) -> Result<ManagedState> {
    let generated = env_set_items(source);
    let stale = previous
        .shell_environment_policy_set
        .iter()
        .map(String::as_str)
        .filter(|key| !generated.iter().any(|(name, _)| name == key));
    remove_env_keys(target, stale);
    for (key, item) in &generated {
        env_set_table(target)?.insert(key, (*item).clone());
    }
    Ok(ManagedState {
        shell_environment_policy_set: generated.iter().map(|(key, _)| key.to_string()).collect(),
    })
}

fn env_set_table(target: &mut DocumentMut) -> Result<&mut dyn TableLike> {
    target
        .entry("shell_environment_policy")
        .or_insert_with(toml_edit::table)
        .as_table_like_mut()
        .context("shell_environment_policy is not a table")?
        .entry("set")
        .or_insert_with(toml_edit::table)
        .as_table_like_mut()
        .context("shell_environment_policy.set is not a table")
}

fn env_set_items(source: &DocumentMut) -> Vec<(&str, &Item)> {
    source
        .get("shell_environment_policy")
        .and_then(|policy| policy.get("set"))
        .and_then(Item::as_table_like)
        .map(|set| set.iter().collect())
        .unwrap_or_default()
}

fn remove_env_keys<'a>(target: &mut DocumentMut, keys: impl Iterator<Item = &'a str>) {
    let set = target
        .get_mut("shell_environment_policy")
        .and_then(|policy| policy.get_mut("set"))
        .and_then(Item::as_table_like_mut);
    let Some(set) = set else {
        return;
    };
    keys.for_each(|key| {
        set.remove(key);
    });
}

fn read_toml_document(path: &Path) -> Result<DocumentMut> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read TOML file {}", path.display()))?;
    content
        .parse::<DocumentMut>()
        .with_context(|| format!("failed to parse TOML file {}", path.display()))
}

fn write_toml_document(path: &Path, document: &DocumentMut) -> Result<()> {
    fs_ops::write_file_atomically(path, document.to_string().as_bytes())
}

#[cfg(test)]
mod tests;
