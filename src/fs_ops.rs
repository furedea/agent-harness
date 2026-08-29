use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

pub(crate) fn copy_dir(source: &Path, target: &Path) -> Result<()> {
    if target.exists() {
        std::fs::remove_dir_all(target)
            .with_context(|| format!("failed to remove directory {}", target.display()))?;
    }
    std::fs::create_dir_all(target)
        .with_context(|| format!("failed to create directory {}", target.display()))?;

    for file in regular_files(source)? {
        let relative = file
            .strip_prefix(source)
            .with_context(|| format!("failed to strip prefix {}", source.display()))?;
        let destination = target.join(relative);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create directory {}", parent.display()))?;
        }
        std::fs::copy(&file, &destination).with_context(|| {
            format!(
                "failed to copy {} to {}",
                file.display(),
                destination.display(),
            )
        })?;
    }

    Ok(())
}

pub(crate) fn copy_file(source: &Path, target: &Path) -> Result<()> {
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory {}", parent.display()))?;
    }
    std::fs::copy(source, target).with_context(|| {
        format!(
            "failed to copy {} to {}",
            source.display(),
            target.display()
        )
    })?;
    Ok(())
}

pub(crate) fn write_regular_file(target: &Path, content: &[u8]) -> Result<()> {
    let temporary = temporary_file(target)?;
    remove_path(&temporary)?;
    std::fs::write(&temporary, content)
        .with_context(|| format!("failed to write {}", temporary.display()))?;
    replace_file(&temporary, target)
}

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
        let path = entry.path();

        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            collect_regular_files(&path, files)?;
        } else if file_type.is_file() {
            files.push(path);
        }
    }

    Ok(())
}

fn remove_path(path: &Path) -> Result<()> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(error).with_context(|| format!("failed to inspect {}", path.display()));
        }
    };

    if metadata.file_type().is_dir() {
        std::fs::remove_dir_all(path)
            .with_context(|| format!("failed to remove directory {}", path.display()))
    } else {
        std::fs::remove_file(path)
            .with_context(|| format!("failed to remove file {}", path.display()))
    }
}

fn temporary_file(target: &Path) -> Result<PathBuf> {
    let parent = target
        .parent()
        .with_context(|| format!("file target has no parent: {}", target.display()))?;
    std::fs::create_dir_all(parent)
        .with_context(|| format!("failed to create directory {}", parent.display()))?;
    let name = target
        .file_name()
        .with_context(|| format!("file target has no name: {}", target.display()))?;
    Ok(parent.join(format!(".{}.agent-harness.tmp", name.to_string_lossy(),)))
}

#[cfg(unix)]
fn replace_file(source: &Path, target: &Path) -> Result<()> {
    std::fs::rename(source, target)
        .with_context(|| format!("failed to replace file {}", target.display()))
}

#[cfg(not(unix))]
fn replace_file(source: &Path, target: &Path) -> Result<()> {
    remove_path(target)?;
    std::fs::rename(source, target)
        .with_context(|| format!("failed to replace file {}", target.display()))
}
