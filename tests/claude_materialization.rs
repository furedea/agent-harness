use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

#[cfg(unix)]
#[test]
fn sync_claude_settings_materializes_a_regular_settings_file() {
    let fixture = ClaudeSettingsSyncFixture::new();
    fixture.sync();

    assert_regular_file(&fixture.target);
    assert_owner_writable(&fixture.target);

    fixture.remove();
}

#[cfg(unix)]
#[test]
fn sync_claude_settings_preserves_provider_owned_settings() {
    let fixture = ClaudeSettingsSyncFixture::new();
    fixture.sync();

    let settings = read_json(&fixture.target);
    assert_eq!(settings["model"], "managed-model");
    assert_eq!(settings["hooks"]["PreToolUse"], json!(["managed-hook"]));
    assert_eq!(settings["hooks"]["ProviderOnly"], json!(["preserved-hook"]),);
    assert_eq!(settings["permissions"]["allow"], json!(["managed-command"]));
    assert_eq!(settings["permissions"]["providerOnly"], true);
    assert_eq!(settings["nested"]["managed"], true);
    assert_eq!(settings["nested"]["provider"], true);
    assert_eq!(settings["providerState"]["account"], "preserved");

    fixture.remove();
}

#[cfg(unix)]
struct ClaudeSettingsSyncFixture {
    root: PathBuf,
    source: PathBuf,
    target: PathBuf,
}

#[cfg(unix)]
impl ClaudeSettingsSyncFixture {
    fn new() -> Self {
        use std::os::unix::fs::{PermissionsExt, symlink};

        let root = test_root();
        let source = root.join("source/settings.json");
        let existing = root.join("old-generation/settings.json");
        let target = root.join("home/.claude/settings.json");

        write_file(
            &source,
            r#"{
  "model": "managed-model",
  "hooks": {"PreToolUse": ["managed-hook"]},
  "permissions": {"allow": ["managed-command"]},
  "nested": {"managed": true}
}
"#,
        );
        std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o444)).unwrap();
        write_file(
            &existing,
            r#"{
  "model": "provider-model",
  "hooks": {
    "PreToolUse": ["provider-hook"],
    "ProviderOnly": ["preserved-hook"]
  },
  "permissions": {
    "allow": ["provider-command"],
    "providerOnly": true
  },
  "nested": {"provider": true},
  "providerState": {"account": "preserved"}
}
"#,
        );
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        symlink(existing, &target).unwrap();

        Self {
            root,
            source,
            target,
        }
    }

    fn sync(&self) {
        run_harness([
            "sync-claude-settings",
            "--source",
            self.source.to_str().unwrap(),
            "--target",
            self.target.to_str().unwrap(),
        ]);
    }

    fn remove(self) {
        std::fs::remove_dir_all(self.root).unwrap();
    }
}

fn test_root() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "agent-harness-claude-materialization-{}-{nanos}",
        std::process::id(),
    ));
    std::fs::create_dir_all(&root).unwrap();
    root
}

fn write_file(path: &Path, content: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

fn run_harness<const N: usize>(args: [&str; N]) {
    let output = Command::new(env!("CARGO_BIN_EXE_agent-harness"))
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "agent-harness failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

fn read_json(path: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn assert_regular_file(path: &Path) {
    let file_type = std::fs::symlink_metadata(path).unwrap().file_type();
    assert!(
        file_type.is_file(),
        "{} is not a regular file",
        path.display()
    );
    assert!(!file_type.is_symlink(), "{} is a symlink", path.display());
}

#[cfg(unix)]
fn assert_owner_writable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;

    let mode = std::fs::metadata(path).unwrap().permissions().mode();
    assert_ne!(mode & 0o200, 0, "{} is not owner-writable", path.display());
}
