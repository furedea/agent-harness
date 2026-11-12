use super::*;
use crate::generation::external_hooks::ExternalHookBundle;
use crate::provider::spec::PROVIDERS;
use crate::shell::{LoadInputs, fixtures::load_source_with};

fn write_config_source(
    source: &Path,
    out: &Path,
    external_hooks: &[ExternalHookBundle],
) -> Result<()> {
    let inputs = LoadInputs {
        hook_bundles: external_hooks,
        ..LoadInputs::new(PROVIDERS)
    };
    let loaded = load_source_with(source, &inputs)?;
    crate::shell::execute(&[config_source_op(&loaded, out)?])
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

#[test]
fn merge_item_keeps_nested_source_tables_under_an_inline_table() -> Result<()> {
    for base in ["permissions = {}\n", "permissions = { other = 1 }\n"] {
        assert_guarded_entry_survives_merge(base)?;
    }
    Ok(())
}

fn assert_guarded_entry_survives_merge(base: &str) -> Result<()> {
    let mut target = base.parse::<DocumentMut>()?;
    let source = "[permissions.guarded.filesystem]\n\"/a\" = \"read\"\n".parse::<DocumentMut>()?;

    for (key, item) in source.iter() {
        merge_item(&mut target[key], item);
    }

    let rendered = target.to_string();
    let reparsed = rendered.parse::<DocumentMut>()?;
    assert_eq!(
        reparsed["permissions"]["guarded"]["filesystem"]["/a"].as_str(),
        Some("read"),
        "{rendered}",
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

    crate::shell::fixtures::remove_dir_all(root)?;
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

    crate::shell::fixtures::remove_dir_all(root)?;
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

    crate::shell::fixtures::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn write_config_source_keeps_a_stricter_source_access_over_a_secret_rule() -> Result<()> {
    let root = test_root("write_config_source_keeps_a_stricter_source_access")?;
    write_minimal_source(&root)?;
    write_file(
        &root.join("codex/config.toml"),
        "[permissions.guarded.filesystem]\n\"~/.aws/credentials\" = \"none\"\n",
    )?;
    write_file(
        &root.join("hooks/rules/secret_path_policy.json"),
        r#"{"version":1,"rules":[{"pattern":"~/.aws/credentials","access":["write"]}]}"#,
    )?;
    let out = root.join("config.toml");

    write_config_source(&root, &out, &[])?;

    let document = std::fs::read_to_string(&out)?.parse::<DocumentMut>()?;
    let guarded = &document["permissions"]["guarded"]["filesystem"];
    assert_eq!(guarded["~/.aws/credentials"].as_str(), Some("none"));

    crate::shell::fixtures::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn write_config_source_keeps_a_scalar_workspace_grant_beside_scoped_secret_rules() -> Result<()> {
    let root = test_root("write_config_source_keeps_a_scalar_workspace_grant")?;
    write_minimal_source(&root)?;
    write_file(
        &root.join("codex/config.toml"),
        "[permissions.guarded.filesystem]\n\":workspace_roots\" = \"write\"\n",
    )?;
    write_file(
        &root.join("hooks/rules/secret_path_policy.json"),
        r#"{"version":1,"rules":[{"pattern":"**/secrets/**","access":["read"]}]}"#,
    )?;
    let out = root.join("config.toml");

    write_config_source(&root, &out, &[])?;

    let document = std::fs::read_to_string(&out)?.parse::<DocumentMut>()?;
    let roots = &document["permissions"]["guarded"]["filesystem"][":workspace_roots"];
    assert_eq!(roots["."].as_str(), Some("write"));
    assert_eq!(roots["**/secrets/**"].as_str(), Some("none"));

    crate::shell::fixtures::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn write_config_source_keeps_stricter_scoped_source_entries_and_scalar_denials() -> Result<()> {
    let root = test_root("write_config_source_keeps_stricter_scoped_source_entries")?;
    write_minimal_source(&root)?;
    write_file(
        &root.join("codex/config.toml"),
        "[permissions.guarded.filesystem.\":workspace_roots\"]\n\"secrets/file\" = \"deny\"\n",
    )?;
    write_file(
        &root.join("hooks/rules/secret_path_policy.json"),
        r#"{"version":1,"rules":[{"pattern":"secrets/file","access":["write"]}]}"#,
    )?;
    let out = root.join("config.toml");

    write_config_source(&root, &out, &[])?;

    let document = std::fs::read_to_string(&out)?.parse::<DocumentMut>()?;
    let roots = &document["permissions"]["guarded"]["filesystem"][":workspace_roots"];
    assert_eq!(roots["secrets/file"].as_str(), Some("deny"));

    write_file(
        &root.join("codex/config.toml"),
        "[permissions.guarded.filesystem]\n\":workspace_roots\" = \"deny\"\n",
    )?;
    write_config_source(&root, &out, &[])?;

    let document = std::fs::read_to_string(&out)?.parse::<DocumentMut>()?;
    let roots = &document["permissions"]["guarded"]["filesystem"][":workspace_roots"];
    assert_eq!(roots["."].as_str(), Some("deny"));
    assert_eq!(roots["secrets/file"].as_str(), Some("deny"));

    crate::shell::fixtures::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn write_config_source_keeps_broad_source_denials_over_specific_secret_rules() -> Result<()> {
    let root = test_root("write_config_source_keeps_broad_source_denials")?;
    write_minimal_source(&root)?;
    write_file(
        &root.join("hooks/rules/secret_path_policy.json"),
        r#"{"version":1,"rules":[
{"pattern":"secrets/file","access":["write"]},
{"pattern":"~/.aws/credentials","access":["write"]}
]}"#,
    )?;
    let out = root.join("config.toml");

    write_file(
        &root.join("codex/config.toml"),
        "[permissions.guarded.filesystem.\":workspace_roots\"]\n\".\" = \"deny\"\n",
    )?;
    write_config_source(&root, &out, &[])?;
    let document = std::fs::read_to_string(&out)?.parse::<DocumentMut>()?;
    let filesystem = &document["permissions"]["guarded"]["filesystem"];
    assert_eq!(
        filesystem[":workspace_roots"]["secrets/file"].as_str(),
        Some("deny")
    );
    assert_eq!(filesystem["~/.aws/credentials"].as_str(), Some("read"));

    write_file(
        &root.join("codex/config.toml"),
        "[permissions.guarded.filesystem]\n\":root\" = \"deny\"\n\"~/.aws\" = \"deny\"\n",
    )?;
    write_config_source(&root, &out, &[])?;
    let document = std::fs::read_to_string(&out)?.parse::<DocumentMut>()?;
    let filesystem = &document["permissions"]["guarded"]["filesystem"];
    assert_eq!(filesystem["~/.aws/credentials"].as_str(), Some("deny"));
    assert_eq!(filesystem["~/.codex/hooks/adapt.sh"].as_str(), Some("deny"));

    crate::shell::fixtures::remove_dir_all(root)?;
    Ok(())
}

fn test_root(name: &str) -> Result<std::path::PathBuf> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let root = std::env::temp_dir().join(format!("agent-harness-{name}-{nanos}"));
    crate::shell::fixtures::create_dir_all(&root)?;
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
    path.parent()
        .map(crate::shell::fixtures::create_dir_all)
        .transpose()?;
    crate::shell::fixtures::write(path, content)?;
    Ok(())
}
