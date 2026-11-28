//! The MCP servers a sync writes beside those the user or the provider's CLI added.

use anyhow::Result;
use serde_json::json;

use super::{Sandbox, SyncedConfig};
use crate::provider::spec::{ProviderId, SyncedFile};

fn mcp_file_of(provider: ProviderId) -> SyncedConfig<'static> {
    SyncedConfig {
        provider: provider.spec(),
        file: SyncedFile::McpServers,
    }
}

/// Syncs `generated` into `installed` twice, the second time with `next` rendered, as the source
/// dropping a server would.
fn sync_mcp_file(
    provider: ProviderId,
    installed: &serde_json::Value,
    generated: [&serde_json::Value; 2],
) -> Result<serde_json::Value> {
    let sandbox = Sandbox::new(&format!("mcp-{provider:?}"))?;
    let target = sandbox.put_json("mcp_config.json", installed)?;
    for (round, rendered) in generated.into_iter().enumerate() {
        let source = sandbox.put_json(&format!("generated-{round}.json"), rendered)?;
        sandbox.sync_file(
            mcp_file_of(provider),
            &super::SyncPaths {
                source: &source,
                target: &target,
            },
        )?;
    }
    sandbox.json("mcp_config.json")
}

#[test]
fn devin_and_antigravity_keep_servers_the_user_added_and_drop_ones_the_source_dropped() -> Result<()>
{
    let installed = json!({"mcpServers": {"mine": {"command": "mine"}}, "other": true});
    let first = json!({"mcpServers": {
        "docs": {"url": "https://docs.example.com/mcp"},
        "github": {"command": "gh-mcp"},
    }});
    let second = json!({"mcpServers": {"github": {"command": "gh-mcp", "args": ["stdio"]}}});

    for provider in [ProviderId::Devin, ProviderId::Antigravity] {
        let synced = sync_mcp_file(provider, &installed, [&first, &second])?;

        assert_eq!(
            synced,
            json!({
                "mcpServers": {
                    "mine": {"command": "mine"},
                    "github": {"command": "gh-mcp", "args": ["stdio"]},
                },
                "other": true,
            }),
            "{provider:?}"
        );
    }
    Ok(())
}

#[test]
fn a_provider_without_its_own_mcp_file_refuses_to_sync_one() -> Result<()> {
    let sandbox = Sandbox::new("mcp-none")?;
    let source = sandbox.put_json("generated.json", &json!({}))?;
    let target = sandbox.path("mcp_config.json");

    let result = sandbox.sync_file(
        mcp_file_of(ProviderId::Pi),
        &super::SyncPaths {
            source: &source,
            target: &target,
        },
    );

    let message = result.map_or_else(|error| format!("{error:#}"), |()| String::new());
    assert!(message.contains("pi"), "{message}");
    assert!(message.contains("MCP"), "{message}");
    Ok(())
}

#[test]
fn hermes_keeps_servers_the_user_added_to_its_config() -> Result<()> {
    let sandbox = Sandbox::new("mcp-hermes")?;
    let target = sandbox.put(
        "config.yaml",
        "model: mine\nmcp_servers:\n  mine:\n    command: mine\n",
    )?;
    let rounds = [
        "plugins:\n  enabled:\n    - agent-harness-hooks\nmcp_servers:\n  docs:\n    url: https://docs.example.com/mcp\n  github:\n    command: gh-mcp\n",
        "plugins:\n  enabled:\n    - agent-harness-hooks\nmcp_servers:\n  github:\n    command: gh-mcp\n",
    ];
    for (round, rendered) in rounds.into_iter().enumerate() {
        let source = sandbox.put(&format!("managed-{round}.yaml"), rendered)?;
        sandbox.sync(
            ProviderId::Hermes,
            &super::SyncPaths {
                source: &source,
                target: &target,
            },
        )?;
    }

    let synced: serde_norway::Value = serde_norway::from_str(&sandbox.text("config.yaml")?)?;
    let servers = &synced["mcp_servers"];
    assert_eq!(servers["mine"]["command"].as_str(), Some("mine"));
    assert_eq!(servers["github"]["command"].as_str(), Some("gh-mcp"));
    assert!(servers.get("docs").is_none(), "{synced:?}");
    assert_eq!(synced["model"].as_str(), Some("mine"));
    Ok(())
}
