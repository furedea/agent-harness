use std::path::Path;

use anyhow::{Context, Result};
use toml_edit::{DocumentMut, Item, TableLike};

use crate::{
    file_op::FileOp,
    generation::{
        codex_managed_state::{
            ManagedState, managed_state_op, managed_state_path, read_managed_state,
        },
        external_hooks::{ExternalHookBundle, OpenBundle},
        protection,
    },
    layout::SourceLayout,
    runtime_root::RuntimeRoot,
    snapshot::{ConfigFiles, Snapshot, Tree},
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

pub(crate) fn config_source_op(
    source: Tree<'_>,
    out: &Path,
    external_hooks: &[ExternalHookBundle],
) -> Result<FileOp> {
    Ok(FileOp::write(
        out,
        config_source_content(source, external_hooks)?,
    ))
}

pub(crate) fn sync_managed_config_ops(config: ConfigFiles<'_>) -> Result<Vec<FileOp>> {
    let source = read_toml_document(config.files, config.generated)?;
    sync_managed_document_ops(&source, config.target, config.files)
}

pub(crate) fn generated_config_source(
    context: crate::generation::GenerationContext<'_>,
) -> Result<DocumentMut> {
    config_source_content_for_runtime(context)?
        .parse::<DocumentMut>()
        .context("failed to parse generated Codex config source")
}

fn config_source_content(
    source: Tree<'_>,
    external_hooks: &[ExternalHookBundle],
) -> Result<String> {
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
    let base_path = SourceLayout::new(source.root()).codex_config();
    let base = source
        .files()
        .read_text(&base_path)
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
        merge_external_config(&mut document, &bundle.open(source.files())?, source.files())?;
    }
    Ok(document.to_string())
}

fn merge_external_config(
    document: &mut DocumentMut,
    bundle: &OpenBundle<'_>,
    files: &Snapshot,
) -> Result<()> {
    let Some(path) = bundle.codex_config_path() else {
        return Ok(());
    };
    let generated = read_toml_document(files, &path)?;
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

/// The config and its sidecar, both of which the sync rewrites.
pub(crate) fn sync_managed_document_ops(
    source: &DocumentMut,
    target_path: &Path,
    files: &Snapshot,
) -> Result<Vec<FileOp>> {
    let mut target = if files.exists(target_path) {
        read_toml_document(files, target_path)?
    } else {
        DocumentMut::new()
    };

    sync_managed_keys(source, &mut target);
    let state_path = managed_state_path(target_path);
    let state = sync_env_keys(source, &mut target, &read_managed_state(files, &state_path))?;

    Ok(vec![
        FileOp::write(target_path, target.to_string()),
        managed_state_op(&state_path, &state)?,
    ])
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

fn read_toml_document(files: &Snapshot, path: &Path) -> Result<DocumentMut> {
    let content = files
        .read_text(path)
        .with_context(|| format!("failed to read TOML file {}", path.display()))?;
    content
        .parse::<DocumentMut>()
        .with_context(|| format!("failed to parse TOML file {}", path.display()))
}

#[cfg(test)]
mod tests;
