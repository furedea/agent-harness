use anyhow::Result;
use serde_json::json;

use super::Sandbox;
use crate::provider::spec::ProviderId;

#[test]
fn devin_replaces_generated_keys_and_only_its_exec_permission_entries() -> Result<()> {
    let sandbox = Sandbox::new("devin-permissions")?;
    let source = sandbox.put_json(
        "generated.json",
        &json!({
            "hooks": {},
            "permissions": {
                "allow": ["Exec(cargo)", "Exec(git status)"],
                "ask": ["Exec(git push)"],
                "deny": ["Exec(curl)"]
            }
        }),
    )?;
    let target = sandbox.put_json(
        "config.json",
        &json!({
            "version": 1,
            "agent": {"model": "swe-2-max"},
            "hooks": {"SessionStart": ["user-hook"]},
            "permissions": {
                "allow": ["Exec(ls)", "Read(**)", "mcp__github__*"],
                "deny": ["Exec(rm)", "Write(.env*)"],
                "userList": ["Exec(kept)"]
            }
        }),
    )?;

    sandbox.sync(ProviderId::Devin, &source, &target)?;

    assert_eq!(
        sandbox.json("config.json")?,
        json!({
            "version": 1,
            "agent": {"model": "swe-2-max"},
            "hooks": {},
            "permissions": {
                "allow": ["Read(**)", "mcp__github__*", "Exec(cargo)", "Exec(git status)"],
                "ask": ["Exec(git push)"],
                "deny": ["Write(.env*)", "Exec(curl)"],
                "userList": ["Exec(kept)"]
            }
        }),
    );
    Ok(())
}

#[test]
fn devin_is_idempotent_and_keeps_permissions_the_source_does_not_declare() -> Result<()> {
    let sandbox = Sandbox::new("devin-idempotent")?;
    let source = sandbox.put_json(
        "generated.json",
        &json!({"permissions": {"allow": ["Exec(cargo)"], "ask": []}}),
    )?;
    let target = sandbox.path("config.json");
    sandbox.sync(ProviderId::Devin, &source, &target)?;
    let first = sandbox.text("config.json")?;

    sandbox.sync(ProviderId::Devin, &source, &target)?;
    assert_eq!(sandbox.text("config.json")?, first);

    let hooks_only = sandbox.put_json("hooks_only.json", &json!({"hooks": {}}))?;
    sandbox.sync(ProviderId::Devin, &hooks_only, &target)?;
    assert_eq!(
        sandbox.json("config.json")?["permissions"],
        json!({"allow": ["Exec(cargo)"], "ask": []}),
    );
    Ok(())
}

#[test]
fn devin_rejects_a_generated_permission_list_that_is_not_an_array() -> Result<()> {
    let sandbox = Sandbox::new("devin-invalid")?;
    let source = sandbox.put_json("generated.json", &json!({"permissions": {"allow": "x"}}))?;

    assert!(
        sandbox
            .sync(ProviderId::Devin, &source, &sandbox.path("config.json"))
            .is_err()
    );
    Ok(())
}
