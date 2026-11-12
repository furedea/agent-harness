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

fn merged_filesystem(source_config: &str, policy: &str) -> Result<DocumentMut> {
    let root = test_root("merged_filesystem")?;
    write_minimal_source(&root)?;
    write_file(&root.join("codex/config.toml"), source_config)?;
    write_file(&root.join("hooks/rules/secret_path_policy.json"), policy)?;
    let out = root.join("config.toml");
    let written = write_config_source(&root, &out, &[]);
    let document =
        written.and_then(|()| Ok(std::fs::read_to_string(&out)?.parse::<DocumentMut>()?));
    crate::shell::fixtures::remove_dir_all(root)?;
    document
}

fn conflict_message(source_config: &str, policy: &str) -> String {
    let error = merged_filesystem(source_config, policy).unwrap_err();
    format!("{error:#}")
}

const NO_RULES: &str = r#"{"version":1,"rules":[]}"#;
const DENY_AWS: &str = r#"{"version":1,"rules":[{"pattern":"~/.aws/**","access":["read"]}]}"#;
const DENY_BASH_HISTORY: &str =
    r#"{"version":1,"rules":[{"pattern":"~/.bash_history","access":["read"]}]}"#;
const DENY_SECRETS: &str =
    r#"{"version":1,"rules":[{"pattern":"**/secrets/**","access":["read"]}]}"#;

fn assert_conflict(message: &str, stricter: &str, looser: &str) {
    assert!(message.contains("guarded filesystem conflict"), "{message}");
    assert!(message.contains(stricter), "{message}");
    assert!(message.contains(looser), "{message}");
}

#[test]
fn a_source_entry_on_the_same_path_with_another_access_is_a_conflict() {
    let message = conflict_message(
        "[permissions.guarded.filesystem]\n\"~/.bash_history\" = \"read\"\n",
        DENY_BASH_HISTORY,
    );

    assert_conflict(
        &message,
        "~/.bash_history = \"none\" (generated)",
        "~/.bash_history = \"read\" (",
    );
    assert!(message.contains("codex/config.toml"), "{message}");
}

#[test]
fn a_source_ancestor_denial_over_a_generated_protected_path_is_a_conflict() {
    let message = conflict_message(
        "[permissions.guarded.filesystem]\n\"~/.config/../.codex\" = \"deny\"\n",
        NO_RULES,
    );

    assert_conflict(
        &message,
        "= \"deny\" (",
        "~/.codex/hooks/adapt.sh = \"read\" (generated)",
    );
}

#[test]
fn a_generated_glob_denial_over_a_source_concrete_path_is_a_conflict() {
    let message = conflict_message(
        "[permissions.guarded.filesystem]\n\"~/.aws/config\" = \"write\"\n",
        DENY_AWS,
    );

    assert_conflict(
        &message,
        "~/.aws/** = \"none\" (generated)",
        "~/.aws/config = \"write\" (",
    );
}

#[test]
fn a_generated_workspace_denial_over_a_scoped_source_entry_is_a_conflict() {
    let message = conflict_message(
        "[permissions.guarded.filesystem.\":workspace_roots\"]\n\"secrets/key\" = \"read\"\n",
        DENY_SECRETS,
    );

    assert_conflict(
        &message,
        "**/secrets/** = \"none\" (generated)",
        ":workspace_roots / secrets/key = \"read\" (",
    );
}

#[test]
fn a_source_root_denial_over_a_generated_protected_path_is_a_conflict() {
    let message = conflict_message(
        "[permissions.guarded.filesystem]\n\":root\" = \"deny\"\n\"~/.codex\" = \"read\"\n",
        NO_RULES,
    );

    assert_conflict(
        &message,
        ":root = \"deny\" (",
        "~/.codex/hooks/adapt.sh = \"read\" (generated)",
    );
}

#[test]
fn a_source_path_that_climbs_above_its_root_is_rejected() {
    let message = conflict_message(
        "[permissions.guarded.filesystem]\n\"~/../x\" = \"deny\"\n",
        NO_RULES,
    );

    assert!(message.contains("~/../x"), "{message}");
}

#[test]
fn entries_that_do_not_overlap_or_agree_on_access_pass() -> Result<()> {
    let document = merged_filesystem(
        "[permissions.guarded.filesystem]\n\
         \":root\" = \"read\"\n\":workspace_roots\" = \"write\"\n\"/tmp\" = \"write\"\n\
         \"~/.docker\" = \"read\"\n\"~/.kube\" = \"deny\"\n",
        r#"{"version":1,"rules":[
{"pattern":"~/.docker/config.json","access":["read"]},
{"pattern":"**/secrets/**","access":["read","write"]},
{"pattern":"**/agent-harness/verification/**","access":["write"]}
]}"#,
    )?;

    let filesystem = &document["permissions"]["guarded"]["filesystem"];
    assert_eq!(filesystem[":workspace_roots"]["."].as_str(), Some("write"));
    assert_eq!(
        filesystem[":workspace_roots"]["**/secrets/**"].as_str(),
        Some("none")
    );
    assert_eq!(filesystem["~/.docker/config.json"].as_str(), Some("none"));
    assert_eq!(filesystem[":root"].as_str(), Some("read"));
    assert!(!document.to_string().contains("verification"));
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
