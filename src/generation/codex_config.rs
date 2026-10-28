use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use toml_edit::{DocumentMut, Item, Table};

use crate::{
    fs_ops,
    generation::{external_hooks::ExternalHookBundle, io, protection},
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
/// Sidecar next to the Codex config recording the `shell_environment_policy.set` keys last
/// written, so a key dropped from the source is removed while keys Codex or the user added stay.
const MANAGED_STATE_FILE: &str = ".agent-harness-managed.json";
/// Managed before the sidecar existed, so it is removed from the source's absence alone.
const LEGACY_MANAGED_ENV_KEYS: &[&str] = &["GH_CONFIG_DIR"];

#[derive(Serialize, Deserialize)]
struct ManagedState {
    #[serde(default)]
    shell_environment_policy_set: BTreeSet<String>,
}

impl ManagedState {
    fn legacy() -> Self {
        Self {
            shell_environment_policy_set: LEGACY_MANAGED_ENV_KEYS
                .iter()
                .map(ToString::to_string)
                .collect(),
        }
    }
}

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
    source: &Path,
    external_hooks: &[ExternalHookBundle],
    runtime_root: &RuntimeRoot,
) -> Result<DocumentMut> {
    config_source_content_for_runtime(source, external_hooks, runtime_root)?
        .parse::<DocumentMut>()
        .context("failed to parse generated Codex config source")
}

fn config_source_content(source: &Path, external_hooks: &[ExternalHookBundle]) -> Result<String> {
    config_source_content_for_runtime(source, external_hooks, &RuntimeRoot::home())
}

fn config_source_content_for_runtime(
    source: &Path,
    external_hooks: &[ExternalHookBundle],
    runtime_root: &RuntimeRoot,
) -> Result<String> {
    let base_path = SourceLayout::new(source).codex_config();
    let base = std::fs::read_to_string(&base_path)
        .with_context(|| format!("failed to read TOML file {}", base_path.display()))?;
    let mut document = base
        .parse::<DocumentMut>()
        .with_context(|| format!("failed to parse TOML file {}", base_path.display()))?;
    // Merge rather than concatenate: the source may already declare guarded entries, such as
    // the agent GitHub token's read denials.
    let protection =
        protection::codex_config_fragment_for_runtime(source, external_hooks, runtime_root)?
            .parse::<DocumentMut>()
            .context("failed to parse generated Codex config")?;
    for (key, item) in protection.iter() {
        merge_item(&mut document[key], item);
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
    match (target.as_table_mut(), source.as_table()) {
        (Some(target), Some(source)) => merge_tables(target, source),
        _ => *target = source.clone(),
    }
}

fn merge_tables(target: &mut Table, source: &Table) {
    for (key, item) in source {
        merge_item(&mut target[key], item);
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
    let state = sync_env_keys(source, &mut target, &read_managed_state(&state_path));

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
) -> ManagedState {
    let generated = env_set_items(source);
    let stale = previous
        .shell_environment_policy_set
        .iter()
        .map(String::as_str)
        .filter(|key| !generated.iter().any(|(name, _)| name == key));
    remove_env_keys(target, stale);
    for (key, item) in &generated {
        target["shell_environment_policy"]["set"][key] = (*item).clone();
    }
    ManagedState {
        shell_environment_policy_set: generated.iter().map(|(key, _)| key.to_string()).collect(),
    }
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

fn managed_state_path(target_path: &Path) -> PathBuf {
    target_path.with_file_name(MANAGED_STATE_FILE)
}

// A missing or unreadable sidecar must not block the sync; it only forgoes removals, apart from
// the keys managed before the sidecar existed.
fn read_managed_state(path: &Path) -> ManagedState {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_else(ManagedState::legacy)
}

fn write_managed_state(path: &Path, state: &ManagedState) -> Result<()> {
    let content = serde_json::to_string_pretty(state)? + "\n";
    fs_ops::write_file_atomically(path, content.as_bytes())
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
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    #[test]
    fn sync_managed_config_preserves_codex_owned_project_state() -> Result<()> {
        let source = r#"
model = "gpt-5.5"

[features]
hooks = true
"#;
        let target = r#"
model = "old"

[projects."/work"]
trust_level = "trusted"
"#;

        let mut merged = target.parse::<DocumentMut>()?;
        let source = source.parse::<DocumentMut>()?;
        sync_managed_keys(&source, &mut merged);

        assert_eq!(merged["model"].as_str(), Some("gpt-5.5"));
        assert_eq!(merged["features"]["hooks"].as_bool(), Some(true));
        assert_eq!(
            merged["projects"]["/work"]["trust_level"].as_str(),
            Some("trusted"),
        );

        Ok(())
    }

    #[test]
    fn sync_managed_document_writes_every_generated_env_key() -> Result<()> {
        let root = test_root("sync_writes_every_generated_env_key")?;
        let target = root.join("config.toml");
        let source = env_source(&[("GH_CONFIG_DIR", "/gh"), ("NVIM_LOG_FILE", "/nvim.log")])?;

        sync_managed_document(&source, &target)?;

        let env = env_keys(&target)?;
        assert_eq!(env.get("GH_CONFIG_DIR").map(String::as_str), Some("/gh"));
        assert_eq!(
            env.get("NVIM_LOG_FILE").map(String::as_str),
            Some("/nvim.log")
        );

        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn sync_managed_document_removes_a_previously_managed_key_that_left_the_source() -> Result<()> {
        let root = test_root("sync_removes_previously_managed_key")?;
        let target = root.join("config.toml");

        sync_managed_document(&env_source(&[("NVIM_LOG_FILE", "/nvim.log")])?, &target)?;
        sync_managed_document(&env_source(&[("OTHER", "1")])?, &target)?;

        let env = env_keys(&target)?;
        assert!(!env.contains_key("NVIM_LOG_FILE"));
        assert_eq!(env.get("OTHER").map(String::as_str), Some("1"));

        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn sync_managed_document_keeps_env_keys_added_by_the_user_or_codex() -> Result<()> {
        let root = test_root("sync_keeps_foreign_env_keys")?;
        let target = root.join("config.toml");
        write_file(
            &target,
            "[shell_environment_policy.set]\nNODE_REPL_TRUSTED_BROWSER_CLIENT_SHA256S = \"abc\"\n",
        )?;

        sync_managed_document(&env_source(&[("NVIM_LOG_FILE", "/nvim.log")])?, &target)?;
        sync_managed_document(&env_source(&[])?, &target)?;

        let env = env_keys(&target)?;
        assert_eq!(
            env.get("NODE_REPL_TRUSTED_BROWSER_CLIENT_SHA256S")
                .map(String::as_str),
            Some("abc"),
        );
        assert!(!env.contains_key("NVIM_LOG_FILE"));

        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn sync_managed_document_without_a_sidecar_removes_only_gh_config_dir() -> Result<()> {
        let root = test_root("sync_without_sidecar")?;
        let target = root.join("config.toml");
        write_file(
            &target,
            "[shell_environment_policy.set]\nGH_CONFIG_DIR = \"/gh\"\nNVIM_LOG_FILE = \"/old\"\n",
        )?;

        sync_managed_document(&env_source(&[])?, &target)?;

        let env = env_keys(&target)?;
        assert!(!env.contains_key("GH_CONFIG_DIR"));
        assert_eq!(env.get("NVIM_LOG_FILE").map(String::as_str), Some("/old"));

        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn sync_managed_document_keeps_a_user_owned_gh_config_dir_once_a_sidecar_exists() -> Result<()>
    {
        let root = test_root("sync_keeps_user_gh_config_dir")?;
        let target = root.join("config.toml");

        sync_managed_document(&env_source(&[])?, &target)?;
        write_file(
            &target,
            "[shell_environment_policy.set]\nGH_CONFIG_DIR = \"/mine\"\n",
        )?;
        sync_managed_document(&env_source(&[])?, &target)?;

        let env = env_keys(&target)?;
        assert_eq!(env.get("GH_CONFIG_DIR").map(String::as_str), Some("/mine"));

        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn sync_managed_document_survives_an_unreadable_sidecar() -> Result<()> {
        let root = test_root("sync_unreadable_sidecar")?;
        let target = root.join("config.toml");
        write_file(&root.join(MANAGED_STATE_FILE), "not json")?;

        sync_managed_document(&env_source(&[("NVIM_LOG_FILE", "/nvim.log")])?, &target)?;

        let env = env_keys(&target)?;
        assert_eq!(
            env.get("NVIM_LOG_FILE").map(String::as_str),
            Some("/nvim.log")
        );
        let sidecar = std::fs::read_to_string(root.join(MANAGED_STATE_FILE))?;
        assert!(sidecar.contains("NVIM_LOG_FILE"));

        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    fn env_source(pairs: &[(&str, &str)]) -> Result<DocumentMut> {
        let header = (!pairs.is_empty()).then_some("[shell_environment_policy.set]\n");
        let entries = pairs
            .iter()
            .map(|(key, value)| format!("{key} = \"{value}\"\n"));
        let content: String = std::iter::once("model = \"gpt-5.5\"\n".to_string())
            .chain(header.map(str::to_string))
            .chain(entries)
            .collect();
        Ok(content.parse::<DocumentMut>()?)
    }

    fn env_keys(target: &Path) -> Result<BTreeMap<String, String>> {
        let document = read_toml_document(target)?;
        let set = document
            .get("shell_environment_policy")
            .and_then(|policy| policy.get("set"))
            .and_then(Item::as_table_like);
        Ok(set
            .into_iter()
            .flat_map(|set| set.iter())
            .filter_map(|(key, item)| Some((key.to_string(), item.as_str()?.to_string())))
            .collect())
    }

    #[test]
    fn write_config_source_appends_guarded_filesystem_fragment() -> Result<()> {
        let root = test_root("write_config_source_appends_guarded_filesystem_fragment")?;
        write_minimal_source(&root)?;
        let out = root.join("config.toml");

        write_config_source(&root, &out, &[])?;

        let content = std::fs::read_to_string(&out)?;
        assert!(content.contains("model = \"gpt-5.5\""));
        assert!(content.contains("[permissions.guarded.filesystem]"));
        assert!(content.contains("\"~/.codex/hooks/adapt.sh\" = \"read\""));

        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn write_config_source_keeps_guarded_filesystem_entries_from_the_source() -> Result<()> {
        let root = test_root("write_config_source_keeps_guarded_filesystem_entries")?;
        write_minimal_source(&root)?;
        write_file(
            &root.join("codex/config.toml"),
            "[permissions.guarded.filesystem]\n\"~/.config/gh/hosts.yml\" = \"deny\"\n",
        )?;
        let out = root.join("config.toml");

        write_config_source(&root, &out, &[])?;

        let document = std::fs::read_to_string(&out)?.parse::<DocumentMut>()?;
        let guarded = &document["permissions"]["guarded"]["filesystem"];
        assert_eq!(guarded["~/.config/gh/hosts.yml"].as_str(), Some("deny"));
        assert_eq!(guarded["~/.codex/hooks/adapt.sh"].as_str(), Some("read"));

        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    fn test_root(name: &str) -> Result<std::path::PathBuf> {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let root = std::env::temp_dir().join(format!("agent-harness-{name}-{nanos}"));
        std::fs::create_dir_all(&root)?;
        Ok(root)
    }

    fn write_minimal_source(source: &Path) -> Result<()> {
        write_file(
            &source.join("manifest.json"),
            r#"{"version":1,"runtime_commands":[]}"#,
        )?;
        write_file(&source.join("codex/config.toml"), "model = \"gpt-5.5\"\n")?;
        write_file(&source.join("AGENTS.md"), "agent instructions\n")?;
        write_file(&source.join("hooks/guard.sh"), "#!/bin/bash\n")?;
        write_file(&source.join("codex/hooks/adapt.sh"), "#!/bin/bash\n")?;
        Ok(())
    }

    fn write_file(path: &Path, content: &str) -> Result<()> {
        path.parent().map(std::fs::create_dir_all).transpose()?;
        std::fs::write(path, content)?;
        Ok(())
    }
}
