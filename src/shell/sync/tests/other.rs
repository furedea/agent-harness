use anyhow::Result;
use serde_json::json;

use super::Sandbox;
use crate::provider::spec::ProviderId;

use crate::shell::fixtures::create_dir_all;

#[test]
fn hermes_unions_enabled_plugins_and_keeps_user_keys() -> Result<()> {
    let sandbox = Sandbox::new("hermes-union")?;
    let source = sandbox.put(
        "managed.yaml",
        "plugins:\n  enabled:\n    - agent-harness-hooks\n",
    )?;
    let target = sandbox.put(
        "config.yaml",
        "model:\n  provider: openai-codex\nplugins:\n  enabled:\n    - moshi-hooks\nonboarding:\n  seen: true\n",
    )?;

    sandbox.sync(ProviderId::Hermes, &source, &target)?;

    let parsed: serde_norway::Value = serde_norway::from_str(&sandbox.text("config.yaml")?)?;
    assert_eq!(parsed["model"]["provider"].as_str(), Some("openai-codex"));
    assert_eq!(parsed["onboarding"]["seen"].as_bool(), Some(true));
    assert_eq!(
        parsed["plugins"]["enabled"],
        serde_norway::from_str::<serde_norway::Value>("- moshi-hooks\n- agent-harness-hooks\n")?,
    );
    Ok(())
}

#[test]
fn hermes_creates_a_missing_target_and_does_not_duplicate_on_repeat() -> Result<()> {
    let sandbox = Sandbox::new("hermes-missing")?;
    let source = sandbox.put(
        "managed.yaml",
        "plugins:\n  enabled:\n    - agent-harness-hooks\n",
    )?;
    let target = sandbox.path("nested/config.yaml");

    sandbox.sync(ProviderId::Hermes, &source, &target)?;
    sandbox.sync(ProviderId::Hermes, &source, &target)?;

    let parsed: serde_norway::Value = serde_norway::from_str(&sandbox.text("nested/config.yaml")?)?;
    assert_eq!(
        parsed["plugins"]["enabled"].as_sequence().map(Vec::len),
        Some(1)
    );
    Ok(())
}

#[test]
fn hermes_treats_an_empty_target_file_as_an_empty_mapping() -> Result<()> {
    let sandbox = Sandbox::new("hermes-empty")?;
    let source = sandbox.put(
        "managed.yaml",
        "plugins:\n  enabled:\n    - agent-harness-hooks\n",
    )?;
    let target = sandbox.put("config.yaml", "")?;

    sandbox.sync(ProviderId::Hermes, &source, &target)?;

    assert!(sandbox.text("config.yaml")?.contains("agent-harness-hooks"));
    Ok(())
}

#[test]
fn pi_replaces_only_the_model_defaults_it_owns() -> Result<()> {
    let sandbox = Sandbox::new("pi-keys")?;
    let source = sandbox.put_json(
        "source.json",
        &json!({
            "defaultProvider": "managed-provider",
            "defaultModel": "managed",
            "theme": "managed-theme"
        }),
    )?;
    let target = sandbox.put_json(
        "settings.json",
        &json!({"defaultThinkingLevel": "high", "defaultModel": "old", "theme": "user"}),
    )?;

    sandbox.sync(ProviderId::Pi, &source, &target)?;

    assert_eq!(
        sandbox.json("settings.json")?,
        json!({
            "defaultProvider": "managed-provider",
            "defaultThinkingLevel": "high",
            "defaultModel": "managed",
            "theme": "user"
        }),
    );
    Ok(())
}

#[test]
fn pi_rejects_invalid_documents_and_leaves_the_target_untouched() -> Result<()> {
    let sandbox = Sandbox::new("pi-invalid")?;
    let source = sandbox.put_json("source.json", &json!({"defaultModel": "managed"}))?;
    let target = sandbox.path("settings.json");
    for invalid in ["invalid JSON", "[]", "null"] {
        sandbox.put("settings.json", invalid)?;
        assert!(sandbox.sync(ProviderId::Pi, &source, &target).is_err());
        assert_eq!(sandbox.text("settings.json")?, invalid);
    }
    Ok(())
}

#[test]
fn dsh_registers_the_bundle_in_an_existing_profile_once() -> Result<()> {
    let sandbox = Sandbox::new("dsh-register")?;
    let bundle = sandbox.root.join("bundle");
    create_dir_all(&bundle)?;
    let target = sandbox.put_json(
        "package.json",
        &json!({
            "dependencies": {"other": "1.0.0"},
            "dsh": {"profile": {"bundles": ["user-bundle"], "other": true}}
        }),
    )?;

    sandbox.sync(ProviderId::Dsh, &bundle, &target)?;
    sandbox.sync(ProviderId::Dsh, &bundle, &target)?;

    let real = std::fs::canonicalize(&bundle)?;
    assert_eq!(
        sandbox.json("package.json")?,
        json!({
            "dependencies": {
                "other": "1.0.0",
                "agent-harness-dsh-hooks": format!("link:{}", real.display())
            },
            "dsh": {"profile": {
                "bundles": ["user-bundle", "agent-harness-dsh-hooks"],
                "other": true
            }}
        }),
    );
    Ok(())
}

#[test]
fn dsh_skips_a_profile_that_does_not_exist_yet() -> Result<()> {
    let sandbox = Sandbox::new("dsh-missing")?;
    let bundle = sandbox.root.join("bundle");
    create_dir_all(&bundle)?;
    let target = sandbox.path("profiles/web/package.json");

    sandbox.sync(ProviderId::Dsh, &bundle, &target)?;

    assert!(!target.exists());
    Ok(())
}

#[test]
fn dsh_rejects_a_profile_whose_bundles_are_not_a_list() -> Result<()> {
    let sandbox = Sandbox::new("dsh-invalid")?;
    let bundle = sandbox.root.join("bundle");
    create_dir_all(&bundle)?;
    let target = sandbox.put_json("package.json", &json!({"dsh": {"profile": {"bundles": 1}}}))?;

    assert!(sandbox.sync(ProviderId::Dsh, &bundle, &target).is_err());
    Ok(())
}
