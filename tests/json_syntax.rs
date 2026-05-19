use std::path::{Path, PathBuf};
use std::process::Command;

#[test]
fn tracked_json_files_are_valid() {
    let invalid = tracked_json_files()
        .into_iter()
        .filter_map(|path| {
            let content = std::fs::read_to_string(&path).unwrap();
            serde_json::from_str::<serde_json::Value>(&content)
                .err()
                .map(|error| format!("{}: {error}", path.display()))
        })
        .collect::<Vec<_>>();

    assert!(invalid.is_empty(), "invalid JSON files: {invalid:#?}");
}

fn tracked_json_files() -> Vec<PathBuf> {
    let output = Command::new("git")
        .args(["ls-files", "*.json"])
        .current_dir(repo_root())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git ls-files failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|path| repo_root().join(path))
        .collect()
}

fn repo_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}
