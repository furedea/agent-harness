use std::env;
use std::error::Error;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const ASSET_DIRS: [&str; 1] = ["profiles"];

type BuildResult<T> = Result<T, Box<dyn Error>>;

#[allow(
    clippy::disallowed_methods,
    reason = "a build script runs before the crate exists and owns its OUT_DIR output"
)]
fn main() -> BuildResult<()> {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let out_path = PathBuf::from(env::var("OUT_DIR")?).join("packaged_assets.rs");
    let mut files = Vec::new();
    for dir in ASSET_DIRS {
        files.extend(collect_files(&manifest_dir, Path::new(dir))?);
    }

    files.sort();
    fs::write(out_path, packaged_assets_source(&manifest_dir, &files)?)?;
    Ok(())
}

fn collect_files(root: &Path, relative_dir: &Path) -> BuildResult<Vec<PathBuf>> {
    println!(
        "cargo:rerun-if-changed={}",
        root.join(relative_dir).display()
    );

    let mut entries = fs::read_dir(root.join(relative_dir))?.collect::<io::Result<Vec<_>>>()?;
    entries.sort_by_key(fs::DirEntry::file_name);

    let mut files = Vec::new();
    for entry in &entries {
        files.extend(collect_entry(
            root,
            &relative_dir.join(entry.file_name()),
            entry,
        )?);
    }
    Ok(files)
}

fn collect_entry(
    root: &Path,
    relative_path: &Path,
    entry: &fs::DirEntry,
) -> BuildResult<Vec<PathBuf>> {
    let file_type = entry.file_type()?;
    if file_type.is_dir() {
        collect_files(root, relative_path)
    } else if file_type.is_file() {
        Ok(vec![relative_path.to_path_buf()])
    } else {
        Ok(Vec::new())
    }
}

fn packaged_assets_source(root: &Path, files: &[PathBuf]) -> BuildResult<String> {
    let mut entries = String::new();
    for file in files {
        entries.push_str(&packaged_file_entry(root, file)?);
    }
    Ok(format!(
        "const PACKAGED_FILES: &[PackagedFile] = &[\n{entries}];\n"
    ))
}

fn packaged_file_entry(root: &Path, file: &Path) -> BuildResult<String> {
    let path = file.to_string_lossy().replace('\\', "/");
    let absolute_path = root.join(file);
    let include_path = absolute_path
        .to_str()
        .ok_or_else(|| format!("asset path is not valid UTF-8: {}", absolute_path.display()))?;
    let mode = file_mode(&absolute_path)?;
    Ok(format!(
        "    PackagedFile {{ path: {path:?}, mode: {mode}, content: include_bytes!({include_path:?}) }},\n"
    ))
}

#[cfg(unix)]
fn file_mode(path: &Path) -> io::Result<u32> {
    use std::os::unix::fs::PermissionsExt;

    Ok(path.metadata()?.permissions().mode() & 0o777)
}

#[cfg(not(unix))]
#[expect(
    clippy::unnecessary_wraps,
    reason = "keeps the signature of the unix implementation"
)]
fn file_mode(_path: &Path) -> io::Result<u32> {
    Ok(0o644)
}
