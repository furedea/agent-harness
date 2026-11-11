use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Lists the regular files under `dir`, sorted, skipping symlinks; a missing `dir` has none.
pub(crate) fn regular_files(dir: &Path) -> Result<Vec<PathBuf>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut files = Vec::new();
    collect_regular_files(dir, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_regular_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir)
        .with_context(|| format!("failed to read directory {}", dir.display()))?
    {
        let entry = entry.with_context(|| format!("failed to read entry in {}", dir.display()))?;
        let file_type = entry
            .file_type()
            .with_context(|| format!("failed to inspect {}", entry.path().display()))?;
        collect_entry(entry.path(), file_type, files)?;
    }

    Ok(())
}

fn collect_entry(
    path: PathBuf,
    file_type: std::fs::FileType,
    files: &mut Vec<PathBuf>,
) -> Result<()> {
    if file_type.is_symlink() {
        return Ok(());
    }
    if file_type.is_dir() {
        return collect_regular_files(&path, files);
    }
    if file_type.is_file() {
        files.push(path);
    }
    Ok(())
}
