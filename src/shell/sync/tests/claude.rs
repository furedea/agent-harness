use anyhow::Result;
use serde_json::json;

use super::Sandbox;
use crate::provider::spec::ProviderId;

#[test]
fn claude_replaces_generated_top_level_keys_and_keeps_the_rest() -> Result<()> {
    let sandbox = Sandbox::new("claude-keys")?;
    let source = sandbox.put_json(
        "generated.json",
        &json!({
            "hooks": {"PreToolUse": ["generated"]},
            "permissions": {"allow": ["generated-command"]}
        }),
    )?;
    let target = sandbox.put_json(
        "settings.json",
        &json!({
            "model": "user-model",
            "hooks": {"PreToolUse": ["user"]},
            "permissions": {"allow": ["user-command"], "userOnly": true}
        }),
    )?;

    sandbox.sync(
        ProviderId::Claude,
        &super::SyncPaths {
            source: &source,
            target: &target,
        },
    )?;

    assert_eq!(
        sandbox.json("settings.json")?,
        json!({
            "model": "user-model",
            "hooks": {"PreToolUse": ["generated"]},
            "permissions": {"allow": ["generated-command"]}
        }),
    );
    Ok(())
}

#[test]
fn claude_creates_a_missing_target_with_every_generated_key() -> Result<()> {
    let sandbox = Sandbox::new("claude-missing")?;
    let generated = json!({"hooks": {}, "permissions": {"allow": []}});
    let source = sandbox.put_json("generated.json", &generated)?;

    sandbox.sync(
        ProviderId::Claude,
        &super::SyncPaths {
            source: &source,
            target: &sandbox.path("home/settings.json"),
        },
    )?;

    assert_eq!(sandbox.json("home/settings.json")?, generated);
    Ok(())
}

#[test]
fn claude_expands_leading_home_prefixes_of_excluded_commands_only() -> Result<()> {
    let sandbox = Sandbox::new("claude-home")?;
    let source = sandbox.put_json(
        "generated.json",
        &json!({
            "sandbox": {
                "excludedCommands": ["~/bin/tool *", "node ~/script", "git *"],
                "filesystem": {"denyWrite": ["~/protected"]}
            },
            "statusLine": {"command": "~/status.sh"}
        }),
    )?;
    let target = sandbox.path("settings.json");

    sandbox.sync(
        ProviderId::Claude,
        &super::SyncPaths {
            source: &source,
            target: &target,
        },
    )?;

    let home = sandbox.root.display();
    assert_eq!(
        sandbox.json("settings.json")?,
        json!({
            "sandbox": {
                "excludedCommands": [format!("{home}/bin/tool *"), "node ~/script", "git *"],
                "filesystem": {"denyWrite": ["~/protected"]}
            },
            "statusLine": {"command": "~/status.sh"}
        }),
    );
    Ok(())
}

#[test]
fn claude_rejects_a_target_that_is_not_an_object() -> Result<()> {
    let sandbox = Sandbox::new("claude-invalid-target")?;
    let source = sandbox.put_json("generated.json", &json!({"hooks": {}}))?;
    let target = sandbox.put("settings.json", "[]")?;

    assert!(
        sandbox
            .sync(
                ProviderId::Claude,
                &super::SyncPaths {
                    source: &source,
                    target: &target
                }
            )
            .is_err()
    );
    assert_eq!(sandbox.text("settings.json")?, "[]");
    Ok(())
}
