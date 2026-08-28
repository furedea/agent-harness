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

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}
