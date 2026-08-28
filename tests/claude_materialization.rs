use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

#[cfg(unix)]
#[test]
fn sync_claude_files_materializes_regular_files() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = ClaudeSyncFixture::new();
    fixture.sync();

    assert_regular_file(&fixture.target.join("CLAUDE.md"));
    assert_regular_file(&fixture.target.join("settings.json"));
    assert_regular_directory(&fixture.target.join("hooks"));
    assert_regular_directory(&fixture.target.join("skills"));
    assert_regular_file(&fixture.target.join("hooks/guard.sh"));
    assert_regular_file(&fixture.target.join("skills/example/SKILL.md"));
    for path in [
        fixture.target.join("CLAUDE.md"),
        fixture.target.join("settings.json"),
        fixture.target.join("hooks/guard.sh"),
        fixture.target.join("skills/example/SKILL.md"),
    ] {
        assert_owner_writable(&path);
    }
    assert!(!fixture.target.join("hooks/stale.sh").exists());
    assert!(!fixture.target.join("skills/stale/SKILL.md").exists());
    assert_ne!(
        std::fs::metadata(fixture.target.join("hooks/guard.sh"))
            .unwrap()
            .permissions()
            .mode()
            & 0o111,
        0,
    );

    fixture.remove();
}

#[cfg(unix)]
#[test]
fn sync_claude_files_preserves_provider_owned_settings() {
    let fixture = ClaudeSyncFixture::new();
    fixture.sync();

    let settings = read_json(&fixture.target.join("settings.json"));
    assert_eq!(settings["model"], "managed-model");
    assert_eq!(settings["hooks"]["PreToolUse"], json!(["managed-hook"]));
    assert_eq!(settings["hooks"]["ProviderOnly"], json!(["preserved-hook"]),);
    assert_eq!(settings["permissions"]["allow"], json!(["managed-command"]),);
    assert_eq!(settings["permissions"]["providerOnly"], true);
    assert_eq!(settings["nested"]["managed"], true);
    assert_eq!(settings["nested"]["provider"], true);
    assert_eq!(settings["providerState"]["account"], "preserved");

    fixture.remove();
}

#[cfg(unix)]
struct ClaudeSyncFixture {
    root: PathBuf,
    source: PathBuf,
    skills_source: PathBuf,
    target: PathBuf,
}

#[cfg(unix)]
impl ClaudeSyncFixture {
    fn new() -> Self {
        use std::os::unix::fs::{PermissionsExt, symlink};

        let root = test_root();
        let source = root.join("source");
        let skills_source = root.join("skills-source");
        let old = root.join("old-generation");
        let target = root.join("home/.claude");

        write_file(&source.join("CLAUDE.md"), "managed instructions\n");
        write_file(
            &source.join("settings.json"),
            r#"{
  "model": "managed-model",
  "hooks": {"PreToolUse": ["managed-hook"]},
  "permissions": {"allow": ["managed-command"]},
  "nested": {"managed": true}
}
"#,
        );
        write_file(
            &source.join("hooks/guard.sh"),
            "#!/usr/bin/env bash\nexit 0\n",
        );
        std::fs::set_permissions(
            source.join("hooks/guard.sh"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        write_file(
            &skills_source.join("example/SKILL.md"),
            "---\nname: example\n---\n",
        );
        for path in [
            source.join("CLAUDE.md"),
            source.join("settings.json"),
            skills_source.join("example/SKILL.md"),
        ] {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o444)).unwrap();
        }
        std::fs::set_permissions(
            source.join("hooks/guard.sh"),
            std::fs::Permissions::from_mode(0o555),
        )
        .unwrap();
        write_file(&old.join("CLAUDE.md"), "old instructions\n");
        write_file(
            &old.join("settings.json"),
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
        write_file(&old.join("hooks/stale.sh"), "stale\n");
        write_file(&old.join("skills/stale/SKILL.md"), "stale\n");
        std::fs::create_dir_all(&target).unwrap();
        symlink(old.join("CLAUDE.md"), target.join("CLAUDE.md")).unwrap();
        symlink(old.join("settings.json"), target.join("settings.json")).unwrap();
        symlink(old.join("hooks"), target.join("hooks")).unwrap();
        symlink(old.join("skills"), target.join("skills")).unwrap();

        Self {
            root,
            source,
            skills_source,
            target,
        }
    }

    fn sync(&self) {
        run_harness([
            "sync-claude-files",
            "--source",
            self.source.to_str().unwrap(),
            "--skills-source",
            self.skills_source.to_str().unwrap(),
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

fn assert_regular_directory(path: &Path) {
    let file_type = std::fs::symlink_metadata(path).unwrap().file_type();
    assert!(file_type.is_dir(), "{} is not a directory", path.display());
    assert!(!file_type.is_symlink(), "{} is a symlink", path.display());
}

#[cfg(unix)]
fn assert_owner_writable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;

    let mode = std::fs::metadata(path).unwrap().permissions().mode();
    assert_ne!(mode & 0o200, 0, "{} is not owner-writable", path.display());
}
