use std::path::Path;

use super::*;

#[test]
fn profile_source_accepts_a_complete_flat_source() {
    let root = flat_source_root("complete");
    write_flat_source(&root, r#"{"version":1,"runtime_commands":[]}"#);

    assert_eq!(profile_source(&root, Profile::Minimal), Some(root.clone()));

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn required_source_rejects_an_unsupported_manifest_version() {
    let root = flat_source_root("unsupported-manifest");
    write_flat_source(&root, r#"{"version":2,"runtime_commands":[]}"#);

    let error = required_source(&root, "--source", Profile::Minimal)
        .err()
        .unwrap();

    assert!(format!("{error:#}").contains("unsupported source manifest version 2"));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn installed_source_dirs_include_release_tarball_layout_next_to_binary() {
    let candidates = installed_source_dirs_for_executable(Path::new(
        "/home/user/.local/agent-harness/agent-harness",
    ));

    assert_eq!(
        candidates,
        vec![
            PathBuf::from("/home/user/.local/agent-harness/share/agent-harness"),
            PathBuf::from("/home/user/.local/share/agent-harness"),
        ],
    );
}

#[test]
fn installed_source_dirs_include_nix_style_prefix_layout() {
    let candidates = installed_source_dirs_for_executable(Path::new(
        "/nix/store/hash-agent-harness/bin/agent-harness",
    ));

    assert!(candidates.contains(&PathBuf::from(
        "/nix/store/hash-agent-harness/share/agent-harness"
    )));
}

fn flat_source_root(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "agent-harness-flat-source-{name}-{}-{}",
        std::process::id(),
        unique_suffix(),
    ))
}

fn write_flat_source(root: &Path, manifest: &str) {
    for relative in [
        "AGENTS.md",
        "command_permissions.json",
        "hooks.json",
        "hooks/rules/allowed_commands.json",
        "hooks/rules/forbidden_commands.json",
        "hooks/rules/secret_commit_policy.json",
        "hooks/rules/secret_path_policy.json",
        "skill_rendering.json",
        "claude/settings.json",
        "codex/config.toml",
    ] {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "fixture\n").unwrap();
    }
    std::fs::write(root.join("manifest.json"), manifest).unwrap();
}
