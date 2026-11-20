use anyhow::Result;
use serde_json::json;

use super::Sandbox;
use crate::provider::spec::ProviderId;

#[test]
fn antigravity_replaces_its_rule_kinds_and_keeps_every_other_setting_and_rule() -> Result<()> {
    let sandbox = Sandbox::new("antigravity-rules")?;
    let source = sandbox.put_json(
        "generated.json",
        &json!({
            "agentMode": "",
            "enableTerminalSandbox": true,
            "permissions": {
                "allow": ["read_file(/nix)", "command(git status)"],
                "deny": ["write_file(~/.gemini/config)"]
            }
        }),
    )?;
    let target = sandbox.put_json(
        "settings.json",
        &json!({
            "modelProvider": "gemini",
            "agentMode": "auto",
            "permissions": {
                "allow": ["read_file(/old)", "mcp(github/get_issue)", "read_url(example.com)"],
                "deny": ["write_file(/stale)", "execute_url(evil.example)"]
            }
        }),
    )?;

    sandbox.sync(
        ProviderId::Antigravity,
        &super::SyncPaths {
            source: &source,
            target: &target,
        },
    )?;

    let home = sandbox.root.display();
    assert_eq!(
        sandbox.json("settings.json")?,
        json!({
            "modelProvider": "gemini",
            "agentMode": "",
            "enableTerminalSandbox": true,
            "permissions": {
                "allow": [
                    "mcp(github/get_issue)",
                    "read_url(example.com)",
                    "read_file(/nix)",
                    "command(git status)"
                ],
                "deny": [
                    "execute_url(evil.example)",
                    format!("write_file({home}/.gemini/config)")
                ]
            }
        }),
    );
    Ok(())
}

#[test]
fn antigravity_expands_the_home_prefix_of_path_rules_only() -> Result<()> {
    let sandbox = Sandbox::new("antigravity-home")?;
    let source = sandbox.put_json(
        "generated.json",
        &json!({
            "permissions": {
                "ask": ["command(~/bin/tool run)"],
                "deny": ["read_file(~/.ssh)", "write_file(/abs/path)"]
            }
        }),
    )?;
    let target = sandbox.path("settings.json");

    sandbox.sync(
        ProviderId::Antigravity,
        &super::SyncPaths {
            source: &source,
            target: &target,
        },
    )?;

    let home = sandbox.root.display();
    assert_eq!(
        sandbox.json("settings.json")?["permissions"],
        json!({
            "ask": [format!("command({home}/bin/tool run)")],
            "deny": [format!("read_file({home}/.ssh)"), "write_file(/abs/path)"]
        }),
    );
    Ok(())
}

#[test]
fn antigravity_applies_source_settings_and_keeps_a_source_rule_once_across_syncs() -> Result<()> {
    let sandbox = Sandbox::new("antigravity-source-settings")?;
    let source = sandbox.put_json(
        "generated.json",
        &json!({
            "modelProvider": "gemini",
            "permissions": {"allow": ["mcp(github/get_issue)", "read_file(/nix)"]}
        }),
    )?;
    let target = sandbox.path("settings.json");
    let paths = super::SyncPaths {
        source: &source,
        target: &target,
    };

    sandbox.sync(ProviderId::Antigravity, &paths)?;
    sandbox.sync(ProviderId::Antigravity, &paths)?;

    assert_eq!(
        sandbox.json("settings.json")?,
        json!({
            "modelProvider": "gemini",
            "permissions": {"allow": ["mcp(github/get_issue)", "read_file(/nix)"]}
        }),
    );
    Ok(())
}

#[test]
fn antigravity_sync_is_idempotent() -> Result<()> {
    let sandbox = Sandbox::new("antigravity-idempotent")?;
    let source = sandbox.put_json(
        "generated.json",
        &json!({"permissions": {"deny": ["write_file(~/.gemini/config)"]}}),
    )?;
    let target = sandbox.path("settings.json");
    let paths = super::SyncPaths {
        source: &source,
        target: &target,
    };
    sandbox.sync(ProviderId::Antigravity, &paths)?;
    let first = sandbox.text("settings.json")?;

    sandbox.sync(ProviderId::Antigravity, &paths)?;

    assert_eq!(sandbox.text("settings.json")?, first);
    Ok(())
}
