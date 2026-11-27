use std::path::{Path, PathBuf};

use anyhow::Result;

use super::super::spec::{McpSpec, Ownership, PROVIDERS, ProviderId};
use crate::shell::fixtures::load_source;

#[test]
fn an_mcp_mapping_in_a_config_is_tracked_so_servers_the_user_added_stay() {
    let mappings = PROVIDERS.iter().filter_map(|spec| match spec.mcp {
        Some(McpSpec::ConfigMapping { path }) => Some((spec, path)),
        Some(McpSpec::SkillsDirectoryPlugin { .. }) | None => None,
    });
    for (spec, path) in mappings {
        let tracked: Vec<_> = spec
            .config
            .unwrap()
            .sync
            .ownership
            .iter()
            .filter_map(tracked_path)
            .collect();
        assert!(tracked.contains(&path), "{} must track {path:?}", spec.key);
    }
}

fn tracked_path(claim: &Ownership) -> Option<&'static [&'static str]> {
    match claim {
        Ownership::TrackedKeys { path, .. } => Some(path),
        Ownership::Keys { .. }
        | Ownership::Entries { .. }
        | Ownership::ListUnion { .. }
        | Ownership::MappingEntries { .. } => None,
    }
}

#[test]
fn an_mcp_plugin_lives_in_a_skills_directory_the_provider_renders() {
    let with_plugin = PROVIDERS
        .iter()
        .filter(|spec| matches!(spec.mcp, Some(McpSpec::SkillsDirectoryPlugin { .. })));
    for spec in with_plugin {
        assert!(spec.skill.is_some(), "{} has no skills directory", spec.key);
    }
    assert!(matches!(
        ProviderId::Claude.spec().mcp,
        Some(McpSpec::SkillsDirectoryPlugin { .. })
    ));
}

fn claude_skills_of(files: &[(&str, &str)]) -> Result<Vec<PathBuf>> {
    let source = crate::shell::TemporaryDirectory::create()?;
    for (path, content) in files {
        let file = source.path().join(path);
        crate::shell::fixtures::create_dir_all(file.parent().unwrap())?;
        crate::shell::fixtures::write(file, content)?;
    }
    let loaded = load_source(source.path())?;
    let rendered = crate::provider::renderer::SkillsDirectory::of(
        ProviderId::Claude.spec(),
        Path::new("skills"),
    )
    .render(&loaded)?;
    let crate::file_op::FileOp::ReplaceTree { contents, .. } = rendered else {
        anyhow::bail!("failed to render the Claude skills: they must replace their directory");
    };
    Ok(contents
        .iter()
        .map(|op| op.target().to_path_buf())
        .collect())
}

#[test]
fn claude_gets_no_mcp_plugin_when_the_source_declares_no_servers() -> Result<()> {
    let without_file = claude_skills_of(&[])?;
    let empty = claude_skills_of(&[("mcp_servers.json", r#"{"servers":{}}"#)])?;

    for targets in [without_file, empty] {
        assert!(
            !targets
                .iter()
                .any(|target| target.starts_with("skills/agent-harness"))
        );
    }
    Ok(())
}

#[test]
fn a_skill_named_like_the_mcp_plugin_is_rejected() {
    let result = claude_skills_of(&[
        ("mcp_servers.json", r#"{"servers":{"x":{"command":"x"}}}"#),
        (
            "skills/agent-harness/SKILL.md",
            "---\nname: agent-harness\ndescription: d\n---\n\n# Agent harness\n",
        ),
    ]);

    let message = result.map_or_else(|error| format!("{error:#}"), |_| String::new());
    assert!(message.contains("MCP server plugin"), "{message}");
}
