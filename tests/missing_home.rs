#![allow(
    clippy::disallowed_methods,
    clippy::disallowed_types,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "integration tests build fixtures on disk and fail fast on any error"
)]

use anyhow::Result;
use std::{path::Path, process::Command};

#[test]
fn install_without_home_fails_and_writes_nothing() -> Result<()> {
    let work_dir = work_dir("install")?;
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/complete-source");

    let output = Command::new(env!("CARGO_BIN_EXE_agent-harness"))
        .arg("install")
        .arg("--source")
        .arg(source)
        .current_dir(&work_dir)
        .env_remove("HOME")
        .output()?;

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("HOME"));
    assert_eq!(std::fs::read_dir(&work_dir)?.count(), 0);
    std::fs::remove_dir_all(work_dir)?;
    Ok(())
}

#[test]
fn sync_config_without_home_fails_and_writes_nothing() -> Result<()> {
    let work_dir = work_dir("sync")?;
    let source = work_dir.join("source.toml");
    std::fs::write(&source, "")?;
    let target = work_dir.join("target.toml");

    let output = Command::new(env!("CARGO_BIN_EXE_agent-harness"))
        .arg("sync-codex-config")
        .arg("--source")
        .arg(&source)
        .arg("--target")
        .arg(&target)
        .current_dir(&work_dir)
        .env_remove("HOME")
        .output()?;

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("HOME"));
    assert!(!target.exists());
    std::fs::remove_dir_all(work_dir)?;
    Ok(())
}

fn work_dir(name: &str) -> Result<std::path::PathBuf> {
    let dir = std::env::temp_dir().join(format!("missing-home-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}
