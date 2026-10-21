use std::path::Path;

use anyhow::{Context, Result};
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
    "notice",
    "tui",
    "plugins",
    "features",
    "default_permissions",
    "permissions",
];
/// Single keys inside Codex-owned tables, so syncing them leaves sibling keys untouched.
const MANAGED_NESTED_KEYS: &[&[&str]] = &[&["shell_environment_policy", "set", "GH_CONFIG_DIR"]];

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
    for path in MANAGED_NESTED_KEYS {
        sync_nested_key(source, &mut target, path);
    }

    write_toml_document(target_path, &target)
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

fn sync_nested_key(source: &DocumentMut, target: &mut DocumentMut, path: &[&str]) {
    let Some((key, tables)) = path.split_last() else {
        return;
    };
    let value = tables
        .iter()
        .try_fold(source.as_item(), |item, table| item.get(table))
        .and_then(|item| item.get(key));
    if let Some(value) = value {
        nested_item_mut(target, tables)[key] = value.clone();
    } else {
        remove_nested_key(target, tables, key);
    }
}

fn nested_item_mut<'a>(target: &'a mut DocumentMut, tables: &[&str]) -> &'a mut Item {
    tables
        .iter()
        .fold(target.as_item_mut(), |item, table| &mut item[table])
}

fn remove_nested_key(target: &mut DocumentMut, tables: &[&str], key: &str) {
    let parent = tables
        .iter()
        .try_fold(target.as_item_mut(), |item, table| item.get_mut(table));
    if let Some(table) = parent.and_then(Item::as_table_like_mut) {
        table.remove(key);
    }
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
