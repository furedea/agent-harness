use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const ASSET_DIRS: [&str; 3] = ["agents", "claude", "codex"];

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap()).join("packaged_assets.rs");
    let mut files = Vec::new();

    for dir in ASSET_DIRS {
        collect_files(&manifest_dir, Path::new(dir), &mut files);
    }

    files.sort();
    fs::write(out_path, packaged_assets_source(&manifest_dir, &files)).unwrap();
}

fn collect_files(root: &Path, relative_dir: &Path, files: &mut Vec<PathBuf>) {
    println!(
        "cargo:rerun-if-changed={}",
        root.join(relative_dir).display()
    );

    let mut entries = fs::read_dir(root.join(relative_dir))
        .unwrap()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        let relative_path = relative_dir.join(entry.file_name());
        let file_type = entry.file_type().unwrap();
        if file_type.is_dir() {
            collect_files(root, &relative_path, files);
        } else if file_type.is_file() {
            files.push(relative_path);
        }
    }
}

fn packaged_assets_source(root: &Path, files: &[PathBuf]) -> String {
    let mut source = String::from("const PACKAGED_FILES: &[PackagedFile] = &[\n");

    for file in files {
        let path = file.to_string_lossy().replace('\\', "/");
        let absolute_path = root.join(file);
        let include_path = absolute_path.to_str().unwrap();
        let mode = file_mode(&absolute_path);
        source.push_str(&format!(
            "    PackagedFile {{ path: {path:?}, mode: {mode}, content: include_bytes!({include_path:?}) }},\n"
        ));
    }

    source.push_str("];\n");
    source
}

#[cfg(unix)]
fn file_mode(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;

    path.metadata().unwrap().permissions().mode() & 0o777
}

#[cfg(not(unix))]
fn file_mode(_path: &Path) -> u32 {
    0o644
}
