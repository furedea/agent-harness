use std::path::PathBuf;
use std::process::Command;

#[test]
fn repository_source_defaults_to_minimal_profile() {
    let output = Command::new(env!("CARGO_BIN_EXE_agent-harness"))
        .args(["list", "skills", "--source", repo_root().to_str().unwrap()])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.starts_with("Skills (0)\n"));
    assert!(!stdout.contains("git-workflow"));
}

#[test]
fn automatically_discovered_source_rejects_an_unsupported_manifest_version() {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("agent-harness-discovered-source-{nanos}"));
    copy_source(&repo_root().join("profiles/minimal"), &root);
    std::fs::write(
        root.join("manifest.json"),
        r#"{"version":2,"runtime_commands":[]}"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_agent-harness"))
        .args(["list", "skills"])
        .current_dir(&root)
        .env_remove("AGENT_HARNESS_SOURCE")
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("unsupported source manifest version 2")
    );
    std::fs::remove_dir_all(root).unwrap();
}

fn copy_source(source: &std::path::Path, target: &std::path::Path) {
    std::fs::create_dir_all(target).unwrap();
    for entry in std::fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let destination = target.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_source(&entry.path(), &destination);
        } else {
            std::fs::copy(entry.path(), destination).unwrap();
        }
    }
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}
