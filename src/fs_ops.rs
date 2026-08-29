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

pub(crate) fn write_file_atomically(target: &Path, content: &[u8]) -> Result<()> {
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory {}", parent.display()))?;
    }

    let temporary = target.with_extension("tmp");
    std::fs::write(&temporary, content)
        .with_context(|| format!("failed to write temporary file {}", temporary.display()))?;
    std::fs::rename(&temporary, target)
        .with_context(|| format!("failed to replace file {}", target.display()))
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

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::symlink;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn atomic_write_replaces_a_symlink_with_a_regular_file() -> Result<()> {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let root = std::env::temp_dir().join(format!("agent-harness-atomic-write-{nanos}"));
        let source = root.join("store/settings.json");
        let target = root.join("home/settings.json");
        std::fs::create_dir_all(source.parent().unwrap())?;
        std::fs::create_dir_all(target.parent().unwrap())?;
        std::fs::write(&source, "store\n")?;
        symlink(&source, &target)?;

        write_file_atomically(&target, b"installed\n")?;

        assert!(std::fs::symlink_metadata(&target)?.file_type().is_file());
        assert_eq!(std::fs::read_to_string(&target)?, "installed\n");
        assert_eq!(std::fs::read_to_string(&source)?, "store\n");
        std::fs::remove_dir_all(root)?;
        Ok(())
    }
}
