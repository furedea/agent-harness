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
    assert_eq!(roots["secrets/file"].as_str(), Some("none"));

    write_file(
        &root.join("codex/config.toml"),
        "[permissions.guarded.filesystem]\n\":workspace_roots\" = \"deny\"\n",
    )?;
    write_config_source(&root, &out, &[])?;

    let document = std::fs::read_to_string(&out)?.parse::<DocumentMut>()?;
    let roots = &document["permissions"]["guarded"]["filesystem"][":workspace_roots"];
    assert_eq!(roots["."].as_str(), Some("deny"));
    assert_eq!(roots["secrets/file"].as_str(), Some("none"));

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
        Some("none")
    );
    assert_eq!(filesystem["~/.aws/credentials"].as_str(), Some("read"));

    write_file(
        &root.join("codex/config.toml"),
        "[permissions.guarded.filesystem]\n\":root\" = \"deny\"\n\"~/.aws\" = \"deny\"\n",
    )?;
    write_config_source(&root, &out, &[])?;
    let document = std::fs::read_to_string(&out)?.parse::<DocumentMut>()?;
    let filesystem = &document["permissions"]["guarded"]["filesystem"];
    assert_eq!(filesystem["~/.aws/credentials"].as_str(), Some("none"));
    assert_eq!(filesystem["~/.codex/hooks/adapt.sh"].as_str(), Some("none"));

    crate::shell::fixtures::remove_dir_all(root)?;
    Ok(())
}

fn merged_filesystem(source_config: &str, policy: &str) -> Result<DocumentMut> {
    let root = test_root("merged_filesystem")?;
    write_minimal_source(&root)?;
    write_file(&root.join("codex/config.toml"), source_config)?;
    write_file(&root.join("hooks/rules/secret_path_policy.json"), policy)?;
    let out = root.join("config.toml");
    write_config_source(&root, &out, &[])?;
    let document = std::fs::read_to_string(&out)?.parse::<DocumentMut>()?;
    crate::shell::fixtures::remove_dir_all(root)?;
    Ok(document)
}

const WRITE_ONLY_SECRETS_FILE: &str =
    r#"{"version":1,"rules":[{"pattern":"secrets/file","access":["write"]}]}"#;
const WRITE_ONLY_AWS_CREDENTIALS: &str =
    r#"{"version":1,"rules":[{"pattern":"~/.aws/credentials","access":["write"]}]}"#;

#[test]
fn a_source_parent_denial_under_workspace_roots_covers_a_generated_child() -> Result<()> {
    let document = merged_filesystem(
        "[permissions.guarded.filesystem.\":workspace_roots\"]\n\"secrets\" = \"deny\"\n",
        WRITE_ONLY_SECRETS_FILE,
    )?;

    let roots = &document["permissions"]["guarded"]["filesystem"][":workspace_roots"];
    assert_eq!(roots["secrets/file"].as_str(), Some("none"));
    Ok(())
}

#[test]
fn a_source_glob_denial_covers_a_generated_concrete_path() -> Result<()> {
    let document = merged_filesystem(
        "[permissions.guarded.filesystem]\n\"~/.aws/**\" = \"deny\"\n",
        WRITE_ONLY_AWS_CREDENTIALS,
    )?;

    let filesystem = &document["permissions"]["guarded"]["filesystem"];
    assert_eq!(filesystem["~/.aws/credentials"].as_str(), Some("none"));
    Ok(())
}

#[test]
fn a_source_path_is_normalized_before_it_is_compared() -> Result<()> {
    let document = merged_filesystem(
        "[permissions.guarded.filesystem]\n\"~/.config/../.aws//\" = \"deny\"\n",
        WRITE_ONLY_AWS_CREDENTIALS,
    )?;

    let filesystem = &document["permissions"]["guarded"]["filesystem"];
    assert_eq!(filesystem["~/.aws/credentials"].as_str(), Some("none"));
    Ok(())
}

#[test]
fn a_source_path_that_climbs_above_its_root_is_rejected() {
    let error = merged_filesystem(
        "[permissions.guarded.filesystem]\n\"~/../x\" = \"deny\"\n",
        WRITE_ONLY_AWS_CREDENTIALS,
    )
    .unwrap_err();

    assert!(format!("{error:#}").contains("~/../x"));
}

#[test]
fn a_source_subtree_read_entry_is_raised_to_the_generated_denial() -> Result<()> {
    let document = merged_filesystem(
        "[permissions.guarded.filesystem]\n\"~/.ssh/**\" = \"read\"\n",
        r#"{"version":1,"rules":[{"pattern":"~/.ssh/**","access":["read"]}]}"#,
    )?;

    let filesystem = &document["permissions"]["guarded"]["filesystem"];
    assert_eq!(filesystem["~/.ssh/**"].as_str(), Some("none"));
    Ok(())
}

#[test]
fn a_source_exception_under_a_broader_source_entry_is_left_alone() -> Result<()> {
    let document = merged_filesystem(
        "[permissions.guarded.filesystem]\n\":root\" = \"read\"\n\":workspace_roots\" = \"write\"\n\"/tmp\" = \"write\"\n",
        r#"{"version":1,"rules":[]}"#,
    )?;

    let filesystem = &document["permissions"]["guarded"]["filesystem"];
    assert_eq!(filesystem[":root"].as_str(), Some("read"));
    assert_eq!(filesystem[":workspace_roots"].as_str(), Some("write"));
    assert_eq!(filesystem["/tmp"].as_str(), Some("write"));
    Ok(())
}

#[test]
fn a_source_slash_tmp_denial_covers_a_generated_path_under_tmp() -> Result<()> {
    let document = merged_filesystem(
        "[permissions.guarded.filesystem]\n\":slash_tmp\" = \"deny\"\n",
        r#"{"version":1,"rules":[{"pattern":"//tmp/secret","access":["write"]}]}"#,
    )?;

    let filesystem = &document["permissions"]["guarded"]["filesystem"];
    assert_eq!(filesystem["/tmp/secret"].as_str(), Some("none"));
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
