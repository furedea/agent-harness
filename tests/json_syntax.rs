use std::path::{Path, PathBuf};

#[test]
fn repository_json_files_are_valid() {
    let invalid = repository_json_files()
        .into_iter()
        .filter_map(|path| {
            let content = std::fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
            serde_json::from_str::<serde_json::Value>(&content)
                .err()
                .map(|error| format!("{}: {error}", path.display()))
        })
        .collect::<Vec<_>>();

    assert!(invalid.is_empty(), "invalid JSON files: {invalid:#?}");
}

fn repository_json_files() -> Vec<PathBuf> {
    let mut paths = top_level_json_files(repo_root());
    for directory in ["agents", "claude", "codex"] {
        collect_json_files(&repo_root().join(directory), &mut paths);
    }
    paths.sort();
    paths
}

fn top_level_json_files(directory: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("failed to read directory {}: {error}", directory.display()))
        .filter_map(|entry| {
            let path = path_from_entry(entry, directory);
            path.is_file()
                .then_some(path)
                .filter(|path| is_json_file(path))
        })
        .collect()
}

fn collect_json_files(directory: &Path, paths: &mut Vec<PathBuf>) {
    if !directory.exists() {
        return;
    }

    for entry in std::fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("failed to read directory {}: {error}", directory.display()))
    {
        let path = path_from_entry(entry, directory);
        if path.is_dir() {
            collect_json_files(&path, paths);
        } else if is_json_file(&path) {
            paths.push(path);
        }
    }
}

fn path_from_entry(entry: std::io::Result<std::fs::DirEntry>, directory: &Path) -> PathBuf {
    entry
        .unwrap_or_else(|error| {
            panic!(
                "failed to read directory entry in {}: {error}",
                directory.display()
            )
        })
        .path()
}

fn is_json_file(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension == "json")
}

fn repo_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}
