use anyhow::Result;
use std::{path::Path, process::Command};

#[test]
fn unified_hook_commands_match_legacy_files() -> Result<()> {
    let root = std::env::temp_dir().join(format!("registry-cli-{}", std::process::id()));
    std::fs::create_dir_all(&root)?;
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/complete-source");
    for provider in ["claude", "codex", "devin", "hermes", "pi", "dsh"] {
        let old = root.join(format!("{provider}-old.json"));
        let new = root.join(format!("{provider}-new.json"));
        for (command, path, unified) in [
            (format!("generate-{provider}-hooks"), &old, false),
            ("generate-hooks".into(), &new, true),
        ] {
            let mut cmd = Command::new(env!("CARGO_BIN_EXE_agent-harness"));
            cmd.arg(command)
                .arg("--source")
                .arg(&source)
                .arg("--output")
                .arg(path);
            if unified {
                cmd.args(["--provider", provider]);
            }
            let output = cmd.output()?;
            assert!(
                output.status.success(),
                "{provider}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        assert_eq!(std::fs::read(old)?, std::fs::read(new)?, "{provider}");
    }
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn unified_sync_commands_match_legacy_files() -> Result<()> {
    let root = std::env::temp_dir().join(format!("registry-sync-{}", std::process::id()));
    std::fs::create_dir_all(&root)?;
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/complete-source");
    let installed = root.join("installed");
    let output = Command::new(env!("CARGO_BIN_EXE_agent-harness"))
        .arg("install")
        .arg("--source")
        .arg(fixture)
        .arg("--prefix")
        .arg(&installed)
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    for (provider, legacy, generated, initial) in [
        (
            "claude",
            "sync-claude-settings",
            ".claude/settings.json",
            "{\"userKey\":true}",
        ),
        (
            "codex",
            "sync-codex-config",
            ".codex/config.toml",
            "model = \"user-model\"\n",
        ),
        (
            "devin",
            "sync-devin-config",
            ".config/devin/config.json",
            "{\"userKey\":true}",
        ),
        (
            "hermes",
            "sync-hermes-config",
            ".hermes/managed-config.yaml",
            "userKey: true\n",
        ),
        (
            "dsh",
            "sync-dsh-config",
            ".dsh/bridge",
            "{\"dependencies\":{\"user\":\"1\"}}",
        ),
    ] {
        let old = root.join(format!("{provider}-old"));
        let new = root.join(format!("{provider}-new"));
        for (command, target, unified) in [(legacy, &old, false), ("sync-config", &new, true)] {
            std::fs::write(target, initial)?;
            let mut cmd = Command::new(env!("CARGO_BIN_EXE_agent-harness"));
            cmd.arg(command)
                .arg("--source")
                .arg(installed.join(generated))
                .arg("--target")
                .arg(target);
            if unified {
                cmd.args(["--provider", provider]);
            }
            let output = cmd.output()?;
            assert!(
                output.status.success(),
                "{provider}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        assert_eq!(std::fs::read(old)?, std::fs::read(new)?, "{provider}");
    }
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn invented_sync_commands_are_rejected() -> Result<()> {
    for command in ["sync-pi-config", "sync-claude-config"] {
        let output = Command::new(env!("CARGO_BIN_EXE_agent-harness"))
            .arg(command)
            .arg("--help")
            .output()?;
        assert!(!output.status.success(), "{command}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("unrecognized subcommand"));
    }
    Ok(())
}
