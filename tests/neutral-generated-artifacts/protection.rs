use std::collections::BTreeSet;
use std::path::Path;

use super::{
    complete_source_root, read_json, read_toml, remove_dir, run_harness, string_set, test_root,
};

#[test]
fn complete_source_keeps_protection_layers_aligned() {
    let root = test_root("protected-paths");
    let prefix = root.join("home");

    run_harness([
        "install",
        "--source",
        complete_source_root().to_str().unwrap(),
        "--prefix",
        prefix.to_str().unwrap(),
    ]);

    let policy = read_json(&prefix.join(".claude/hooks/rules/protected_paths.json"));
    let settings = read_json(&prefix.join(".claude/settings.json"));
    let policy_paths = string_set(&policy["paths"]);
    let deny_write = string_set(&settings["sandbox"]["filesystem"]["denyWrite"]);

    assert_eq!(policy["version"], 1);
    assert_eq!(policy_paths, deny_write);
    assert!(policy_paths.contains("~/.claude/hooks/guard.sh"));
    assert!(policy_paths.contains("~/.codex/hooks/adapt.sh"));
    assert!(!policy_paths.iter().any(|path| path.starts_with('/')));

    remove_dir(root);
}

#[test]
fn complete_source_protects_every_rendered_provider_file_and_sync_target() {
    let root = test_root("protected-rendered-files");
    let prefix = root.join("home");

    run_harness([
        "install",
        "--source",
        complete_source_root().to_str().unwrap(),
        "--prefix",
        prefix.to_str().unwrap(),
    ]);

    let policy_paths =
        string_set(&read_json(&prefix.join(".claude/hooks/rules/protected_paths.json"))["paths"]);
    let settings = read_json(&prefix.join(".claude/settings.json"));
    let deny_edit = string_set(&settings["permissions"]["deny"]);
    let codex_config = read_toml(&prefix.join(".codex/config.toml"));
    let guarded = &codex_config["permissions"]["guarded"]["filesystem"];
    let mut expected = rendered_harness_files(&prefix);
    expected.extend(
        [
            "~/.claude/settings.json",
            "~/.codex/config.toml",
            "~/.config/devin/config.json",
            "~/.hermes/config.yaml",
        ]
        .map(str::to_owned),
    );

    assert!(expected.contains("~/.claude/hooks/lib/provider_adapter.py"));
    assert!(expected.contains("~/.pi/agent/extensions/hook_bridge.ts"));
    for path in &expected {
        assert!(policy_paths.contains(path), "{path} is not protected");
        assert!(
            deny_edit.contains(&format!("Edit({path})")),
            "{path} is not denied"
        );
        assert_eq!(
            guarded[path.as_str()].as_str(),
            Some("read"),
            "{path} is not guarded"
        );
    }

    remove_dir(root);
}

fn rendered_harness_files(prefix: &Path) -> BTreeSet<String> {
    let skills = [prefix.join(".claude/skills"), prefix.join(".codex/skills")];
    let mut pending = vec![prefix.to_path_buf()];
    let mut files = BTreeSet::new();
    while let Some(dir) = pending.pop() {
        let (dirs, regular_files): (Vec<_>, Vec<_>) = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| !skills.contains(path))
            .partition(|path| path.is_dir());
        pending.extend(dirs);
        files.extend(regular_files.iter().map(|path| home_path(prefix, path)));
    }
    files
}

fn home_path(prefix: &Path, path: &Path) -> String {
    let relative = path.strip_prefix(prefix).unwrap().to_str().unwrap();
    format!("~/{relative}")
}
