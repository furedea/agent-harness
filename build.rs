use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const ASSET_DIRS: [&str; 1] = ["profiles"];

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap()).join("packaged_assets.rs");
    let mut files = ASSET_DIRS
        .iter()
        .flat_map(|dir| collect_files(&manifest_dir, Path::new(dir)))
        .collect::<Vec<_>>();

    files.sort();
    fs::write(out_path, packaged_assets_source(&manifest_dir, &files)).unwrap();
}

fn collect_files(root: &Path, relative_dir: &Path) -> Vec<PathBuf> {
    println!(
        "cargo:rerun-if-changed={}",
        root.join(relative_dir).display()
    );

    let mut entries = fs::read_dir(root.join(relative_dir))
        .unwrap()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    entries.sort_by_key(fs::DirEntry::file_name);

    entries
        .iter()
        .flat_map(|entry| collect_entry(root, &relative_dir.join(entry.file_name()), entry))
        .collect()
}

fn collect_entry(root: &Path, relative_path: &Path, entry: &fs::DirEntry) -> Vec<PathBuf> {
    let file_type = entry.file_type().unwrap();
    if file_type.is_dir() {
        collect_files(root, relative_path)
    } else if file_type.is_file() {
        vec![relative_path.to_path_buf()]
    } else {
        Vec::new()
    }
}

fn packaged_assets_source(root: &Path, files: &[PathBuf]) -> String {
    let entries = files
        .iter()
        .map(|file| packaged_file_entry(root, file))
        .collect::<String>();
    format!("const PACKAGED_FILES: &[PackagedFile] = &[\n{entries}];\n")
}

fn packaged_file_entry(root: &Path, file: &Path) -> String {
    let path = file.to_string_lossy().replace('\\', "/");
    let absolute_path = root.join(file);
    let include_path = absolute_path.to_str().unwrap();
    let mode = file_mode(&absolute_path);
    format!(
        "    PackagedFile {{ path: {path:?}, mode: {mode}, content: include_bytes!({include_path:?}) }},\n"
    )
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
