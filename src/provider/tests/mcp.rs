use std::path::{Path, PathBuf};

use anyhow::Result;

use super::super::spec::{
    McpEntry, McpSpec, PROVIDERS, ProviderId, ProviderSpec, RenderedConfig, WhenMissing,
};
use crate::shell::fixtures::load_source;

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

#[test]
fn every_provider_with_an_mcp_client_the_harness_can_reach_gets_the_servers() {
    let rendered: Vec<_> = PROVIDERS
        .iter()
        .filter(|spec| spec.mcp.is_some() || spec.mcp_mapping().is_some())
        .map(|spec| spec.id)
        .collect();

    // Pi has no MCP client, and dsh reaches servers only through a plugin the harness does not
    // install.
    assert_eq!(
        rendered,
        [
            ProviderId::Codex,
            ProviderId::Claude,
            ProviderId::Devin,
            ProviderId::Hermes,
            ProviderId::Antigravity,
        ]
    );
}

#[test]
fn an_mcp_file_of_its_own_claims_nothing_but_its_servers() {
    for (config, path) in PROVIDERS.iter().filter_map(ProviderSpec::mcp_file) {
        assert_eq!(config.rendered, RenderedConfig::McpServers, "{path}");
        assert!(config.sync.ownership.is_empty(), "{path}");
        assert!(config.sync.mcp_servers.is_some(), "{path}");
        assert_eq!(config.sync.when_missing, WhenMissing::Create, "{path}");
    }
}

#[test]
fn no_two_synced_files_in_one_directory_share_a_sidecar() {
    let mut sidecars = std::collections::BTreeSet::new();
    let tracked = PROVIDERS
        .iter()
        .flat_map(ProviderSpec::synced_files)
        .filter_map(|(config, path)| config.sync.sidecar_file().map(|sidecar| (path, sidecar)));
    for (path, sidecar) in tracked {
        let directory = Path::new(path).parent();
        assert!(
            sidecars.insert((directory, sidecar)),
            "{path} shares its sidecar with another synced file"
        );
    }
}

#[test]
fn each_provider_reads_a_remote_server_in_its_own_form() {
    let entry = |id: ProviderId| {
        let spec = id.spec();
        let own_file = spec
            .mcp_file()
            .and_then(|(config, _)| config.sync.mcp_servers);
        spec.mcp_mapping().or(own_file).map(|mapping| mapping.entry)
    };

    assert_eq!(entry(ProviderId::Codex), Some(McpEntry::Url));
    assert_eq!(entry(ProviderId::Hermes), Some(McpEntry::Url));
    assert_eq!(entry(ProviderId::Devin), Some(McpEntry::UrlWithTransport));
    assert_eq!(entry(ProviderId::Antigravity), Some(McpEntry::ServerUrl));
}
