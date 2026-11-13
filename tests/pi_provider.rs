#![allow(
    clippy::disallowed_methods,
    clippy::disallowed_types,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "integration tests build fixtures on disk and fail fast on any error"
)]

use anyhow::Result;
use serde_json::{Value, json};
use std::{path::Path, process::Command};

#[test]
fn pi_install_delivers_claude_skills_and_instructions() -> Result<()> {
    let root = std::env::temp_dir().join(format!("pi-delivery-{}", std::process::id()));
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/complete-source");
    let output = Command::new(env!("CARGO_BIN_EXE_agent-harness"))
        .arg("install")
        .arg("--source")
        .arg(source)
        .arg("--prefix")
        .arg(&root)
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    for (pi, claude) in [
        (".pi/agent/AGENTS.md", ".claude/CLAUDE.md"),
        (
            ".pi/agent/skills/example-skill/SKILL.md",
            ".claude/skills/example-skill/SKILL.md",
        ),
    ] {
        assert_eq!(
            std::fs::read(root.join(pi))?,
            std::fs::read(root.join(claude))?
        );
    }
    let skill = std::fs::read_to_string(root.join(".pi/agent/skills/example-skill/SKILL.md"))?;
    assert!(skill.contains("disable-model-invocation: true"));
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn pi_settings_sync_manages_only_model_defaults_and_is_idempotent() -> Result<()> {
    let root = std::env::temp_dir().join(format!("pi-settings-{}", std::process::id()));
    std::fs::create_dir_all(&root)?;
    let source = root.join("generated.json");
    let target = root.join("settings.json");
    std::fs::write(
        &source,
        serde_json::to_vec(&json!({
            "defaultProvider": "anthropic", "defaultModel": "model-new",
            "defaultThinkingLevel": "high", "theme": "source-theme",
            "permissions": {"allow": []}
        }))?,
    )?;
    for initial in [
        json!({}),
        json!({"theme": "user-theme", "defaultModel": "old", "extensions": ["custom"], "permissions": {"custom": true}}),
    ] {
        std::fs::write(&target, serde_json::to_vec(&initial)?)?;
        let mut expected = initial.clone();
        expected["defaultProvider"] = json!("anthropic");
        expected["defaultModel"] = json!("model-new");
        expected["defaultThinkingLevel"] = json!("high");
        assert_synced_settings(&source, &target, &expected)?;
        assert_synced_settings(&source, &target, &expected)?;
    }
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn pi_install_merges_optional_source_settings_without_losing_user_state() -> Result<()> {
    let root = std::env::temp_dir().join(format!("pi-install-settings-{}", std::process::id()));
    let source = root.join("source");
    std::fs::create_dir_all(&source)?;
    let output = Command::new("cp")
        .arg("-R")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/complete-source/."))
        .arg(&source)
        .output()?;
    assert!(output.status.success());
    let prefix = root.join("home");
    let target = prefix.join(".pi/agent/settings.json");
    std::fs::create_dir_all(target.parent().unwrap())?;
    std::fs::write(
        &target,
        r#"{"theme":"user","defaultModel":"existing","defaultProvider":"keep"}"#,
    )?;
    for settings in [
        None,
        Some(json!({"defaultModel":"managed","theme":"source"})),
    ] {
        let content = settings.as_ref().map(serde_json::to_vec).transpose()?;
        content
            .map(|content| std::fs::write(source.join("pi/settings.json"), content))
            .transpose()?;
        let output = Command::new(env!("CARGO_BIN_EXE_agent-harness"))
            .arg("install")
            .arg("--source")
            .arg(&source)
            .arg("--prefix")
            .arg(&prefix)
            .output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let actual: Value = serde_json::from_slice(&std::fs::read(&target)?)?;
        assert_eq!(
            actual,
            json!({"theme":"user", "defaultModel": if settings.is_some() {"managed"} else {"existing"}, "defaultProvider":"keep"})
        );
    }
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn pi_bridge_enforces_hook_decisions() -> Result<()> {
    let output = Command::new("node")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/pi_bridge_smoke.mjs"))
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

#[test]
fn pi_sync_creates_missing_settings_and_preserves_invalid_target() -> Result<()> {
    let root = std::env::temp_dir().join(format!("pi-sync-boundaries-{}", std::process::id()));
    std::fs::create_dir_all(&root)?;
    let source = root.join("source.json");
    let target = root.join("nested/settings.json");
    std::fs::write(&source, r#"{"defaultModel":"managed"}"#)?;
    let sync = || {
        Command::new(env!("CARGO_BIN_EXE_agent-harness"))
            .args(["sync-config", "--provider", "pi", "--source"])
            .arg(&source)
            .arg("--target")
            .arg(&target)
            .output()
    };
    assert!(sync()?.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&std::fs::read(&target)?)?,
        json!({"defaultModel":"managed"})
    );
    for invalid in ["invalid JSON", "[]", "null"] {
        std::fs::write(&target, invalid)?;
        assert!(!sync()?.status.success());
        assert_eq!(std::fs::read_to_string(&target)?, invalid);
    }
    std::fs::write(&target, r#"{"theme":"user"}"#)?;
    for invalid in ["invalid JSON", "[]", "null"] {
        std::fs::write(&source, invalid)?;
        assert!(!sync()?.status.success());
        assert_eq!(std::fs::read_to_string(&target)?, r#"{"theme":"user"}"#);
    }
    std::fs::remove_dir_all(root)?;
    Ok(())
}

fn assert_synced_settings(source: &Path, target: &Path, expected: &Value) -> Result<()> {
    let output = Command::new(env!("CARGO_BIN_EXE_agent-harness"))
        .args(["sync-config", "--provider", "pi", "--source"])
        .arg(source)
        .arg("--target")
        .arg(target)
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual: Value = serde_json::from_slice(&std::fs::read(target)?)?;
    assert_eq!(&actual, expected);
    Ok(())
}

fn installed_srt_policy(root: &Path, sockets: &[&str]) -> Result<Value> {
    install_and_read_srt_policy(
        root,
        &SandboxSource {
            sockets,
            deny_read: &["~/.ssh"],
            secret_rule: None,
        },
    )
}

/// What the Claude source declares for the sandbox: the unix sockets, `denyRead`, and, when
/// given, one secret path rule `(pattern, access)` in place of the fixture's.
struct SandboxSource<'a> {
    sockets: &'a [&'a str],
    deny_read: &'a [&'a str],
    secret_rule: Option<(&'a str, &'a str)>,
}

fn install_and_read_srt_policy(root: &Path, sandbox: &SandboxSource<'_>) -> Result<Value> {
    let SandboxSource {
        sockets,
        deny_read: source_deny_read,
        secret_rule,
    } = *sandbox;
    let source = root.join("source");
    std::fs::create_dir_all(&source)?;
    assert!(
        Command::new("cp")
            .arg("-R")
            .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/complete-source/."))
            .arg(&source)
            .status()?
            .success()
    );
    let mut claude: Value =
        serde_json::from_slice(&std::fs::read(source.join("claude/settings.json"))?)?;
    claude["sandbox"] = json!({
        "network": {"allowedDomains": ["github.com"], "allowUnixSockets": sockets},
        "filesystem": {"denyRead": source_deny_read, "denyWrite": ["~/**/.git/hooks"], "allowWrite": ["~/.cache/nix"]}
    });
    std::fs::write(
        source.join("claude/settings.json"),
        serde_json::to_vec(&claude)?,
    )?;
    if let Some((pattern, access)) = secret_rule {
        std::fs::write(
            source.join("hooks/rules/secret_path_policy.json"),
            serde_json::to_vec(&json!({
                "version": 1,
                "rules": [{"pattern": pattern, "access": [access]}]
            }))?,
        )?;
    }
    let output = Command::new(env!("CARGO_BIN_EXE_agent-harness"))
        .args(["install", "--source"])
        .arg(&source)
        .arg("--prefix")
        .arg(root.join("home"))
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(serde_json::from_slice(&std::fs::read(
        root.join("home/.pi/agent/extensions/sandbox_policy.json"),
    )?)?)
}

#[test]
fn pi_install_generates_complete_srt_policy_from_claude_source() -> Result<()> {
    let root = std::env::temp_dir().join(format!("pi-sandbox-{}", std::process::id()));
    let policy = installed_srt_policy(&root, &["/custom/socket"])?;
    assert_eq!(policy["network"]["allowedDomains"], json!(["github.com"]));
    assert_eq!(policy["network"]["deniedDomains"], json!([]));
    assert_eq!(
        policy["network"]["allowUnixSockets"],
        json!(["/custom/socket", "/nix/var/nix/daemon-socket/socket"])
    );
    assert!(
        policy["filesystem"]["denyRead"]
            .as_array()
            .unwrap()
            .contains(&json!("~/.pi/agent/auth.json"))
    );
    assert!(
        policy["filesystem"]["denyRead"]
            .as_array()
            .unwrap()
            .contains(&json!("~/.ssh"))
    );
    assert!(
        policy["filesystem"]["denyRead"]
            .as_array()
            .unwrap()
            .contains(&json!("**/secrets/**"))
    );
    assert!(
        policy["filesystem"]["denyWrite"]
            .as_array()
            .unwrap()
            .contains(&json!("~/**/.git/hooks"))
    );
    assert_eq!(policy["filesystem"]["allowWrite"], json!(["~/.cache/nix"]));
    assert!(
        root.join("home/.pi/agent/extensions/bash_sandbox.ts")
            .is_file()
    );
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn pi_srt_policy_lists_the_nix_socket_once_when_the_source_has_it() -> Result<()> {
    let root = std::env::temp_dir().join(format!("pi-sandbox-dedupe-{}", std::process::id()));
    let policy = installed_srt_policy(&root, &["/nix/var/nix/daemon-socket/socket"])?;
    assert_eq!(
        policy["network"]["allowUnixSockets"],
        json!(["/nix/var/nix/daemon-socket/socket"])
    );
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn pi_srt_policy_lists_a_read_denied_path_once_when_the_source_has_it() -> Result<()> {
    let root = std::env::temp_dir().join(format!("pi-sandbox-denyread-{}", std::process::id()));
    let policy = install_and_read_srt_policy(
        &root,
        &SandboxSource {
            sockets: &[],
            deny_read: &["**/secrets/**"],
            secret_rule: None,
        },
    )?;
    let deny_read = policy["filesystem"]["denyRead"].as_array().unwrap();
    assert_eq!(
        deny_read
            .iter()
            .filter(|path| *path == &json!("**/secrets/**"))
            .count(),
        1
    );
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn pi_srt_policy_omits_write_only_secret_rules() -> Result<()> {
    let root = std::env::temp_dir().join(format!("pi-sandbox-writeonly-{}", std::process::id()));
    let policy = install_and_read_srt_policy(
        &root,
        &SandboxSource {
            sockets: &[],
            deny_read: &[],
            secret_rule: Some(("**/audit/**", "write")),
        },
    )?;
    let deny_read = policy["filesystem"]["denyRead"].as_array().unwrap();
    assert!(!deny_read.iter().any(|path| path == &json!("**/audit/**")));
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn pi_bash_tool_runs_only_through_srt() -> Result<()> {
    let output = Command::new("node")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/pi_sandbox_smoke.mjs"))
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}
