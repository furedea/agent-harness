use std::collections::BTreeMap;

use super::*;
use crate::generation::codex_managed_state::MANAGED_STATE_FILE;

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
fn sync_managed_document_writes_into_inline_tables() -> Result<()> {
    let root = test_root("sync_inline_tables")?;
    let target = root.join("config.toml");
    write_file(
        &target,
        "shell_environment_policy = { set = { CI = \"1\" } }\n",
    )?;

    sync_managed_document(&env_source(&[("NVIM_LOG_FILE", "/nvim.log")])?, &target)?;

    let env = env_keys(&target)?;
    assert_eq!(env.get("CI").map(String::as_str), Some("1"));
    assert_eq!(
        env.get("NVIM_LOG_FILE").map(String::as_str),
        Some("/nvim.log")
    );

    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn sync_managed_document_rejects_a_non_table_shell_environment_policy() -> Result<()> {
    let root = test_root("sync_non_table_policy")?;
    let target = root.join("config.toml");
    write_file(&target, "shell_environment_policy = \"inherit\"\n")?;

    let result = sync_managed_document(&env_source(&[("NVIM_LOG_FILE", "/x")])?, &target);

    assert!(result.is_err());
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
fn sync_managed_document_keeps_a_user_owned_gh_config_dir_once_a_sidecar_exists() -> Result<()> {
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

#[test]
fn merge_item_keeps_existing_entries_of_an_inline_table() -> Result<()> {
    let mut target = "features = { existing = true }\n".parse::<DocumentMut>()?;
    let source = "[features]\nadded = true\n".parse::<DocumentMut>()?;

    for (key, item) in source.iter() {
        merge_item(&mut target[key], item);
    }

    let features = target["features"]
        .as_table_like()
        .context("features should stay a table")?;
    assert_eq!(features.get("existing").and_then(Item::as_bool), Some(true));
    assert_eq!(features.get("added").and_then(Item::as_bool), Some(true));
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

#[test]
fn write_config_source_renders_source_declared_write_denied_paths() -> Result<()> {
    let root = test_root("write_config_source_renders_source_declared_write_denied_paths")?;
    write_minimal_source(&root)?;
    write_file(
        &root.join("codex/config.toml"),
        "[permissions.guarded.filesystem]\n\"~/.local/state/agent-harness/verification\" = \"read\"\n",
    )?;
    let out = root.join("config.toml");

    write_config_source(&root, &out, &[])?;

    let document = std::fs::read_to_string(&out)?.parse::<DocumentMut>()?;
    let guarded = &document["permissions"]["guarded"]["filesystem"];
    assert_eq!(
        guarded["~/.local/state/agent-harness/verification"].as_str(),
        Some("read"),
    );
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
