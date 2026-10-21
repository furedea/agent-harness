use std::fs::{File, OpenOptions};
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

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
        copy_file(&file, &target.join(relative))?;
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

    let mut temporary = TemporaryFile::create(target)?;
    temporary
        .file
        .write_all(content)
        .with_context(|| format!("failed to write temporary file for {}", target.display()))?;
    temporary.replace(target)
}

struct TemporaryFile {
    path: PathBuf,
    file: File,
    replaced: bool,
}

impl TemporaryFile {
    fn create(target: &Path) -> Result<Self> {
        let (path, file) = create_temporary_file(target)?;
        Ok(Self {
            path,
            file,
            replaced: false,
        })
    }

    fn replace(mut self, target: &Path) -> Result<()> {
        std::fs::rename(&self.path, target)
            .with_context(|| format!("failed to replace file {}", target.display()))?;
        self.replaced = true;
        Ok(())
    }
}

impl Drop for TemporaryFile {
    fn drop(&mut self) {
        remove_unreplaced_temporary_file(self);
    }
}

fn create_temporary_file(target: &Path) -> Result<(PathBuf, File)> {
    static NEXT_ID: AtomicU64 = AtomicU64::new(0);
    let name = target.file_name().context("output path must name a file")?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    let failure = format!("failed to create temporary file for {}", target.display());
    loop {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let mut temporary_name = name.to_os_string();
        temporary_name.push(format!(".{}.{id}.tmp", std::process::id()));
        let path = target.with_file_name(temporary_name);
        match options.open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(error) => return Err(error).context(failure),
        }
    }
}

fn remove_unreplaced_temporary_file(temporary: &TemporaryFile) {
    if !temporary.replaced {
        let _ = std::fs::remove_file(&temporary.path);
    }
}

pub(crate) struct TemporaryDirectory {
    path: PathBuf,
}

impl TemporaryDirectory {
    pub(crate) fn create() -> Result<Self> {
        create_staging_directory().map(|path| Self { path })
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn create_staging_directory() -> Result<PathBuf> {
    static NEXT_ID: AtomicU64 = AtomicU64::new(0);
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    builder.mode(0o700);
    loop {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("agent-harness-staging-{}-{id}", std::process::id()));
        let failure = format!("failed to create staging directory {}", path.display());
        match builder.create(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(error) => return Err(error).context(failure),
        }
    }
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

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::symlink;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn atomic_write_preserves_an_unrelated_temporary_sibling() -> Result<()> {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let root = std::env::temp_dir().join(format!("agent-harness-atomic-sibling-{nanos}"));
        std::fs::create_dir_all(&root)?;
        let target = root.join("settings.json");
        let sibling = root.join("settings.tmp");
        std::fs::write(&sibling, "unrelated\n")?;

        write_file_atomically(&target, b"installed\n")?;

        assert_eq!(std::fs::read_to_string(&sibling)?, "unrelated\n");
        assert_eq!(std::fs::read_to_string(&target)?, "installed\n");
        std::fs::remove_dir_all(root)?;
        Ok(())
    }

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
