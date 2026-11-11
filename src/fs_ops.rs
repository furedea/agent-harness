use std::fs::{File, OpenOptions};
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result, ensure};

/// Copies a source directory over `target` so that a failure leaves `target` untouched.
pub(crate) fn copy_dir(source: &Path, target: &Path) -> Result<()> {
    create_parent(target)?;
    let staged = stage_directory(source, target)?;
    let backup = swap_in(&staged, target);
    let _ = remove_path(&staged);
    backup?.as_deref().map_or(Ok(()), discard)
}

/// Copies `source` into a fresh sibling of `target`, which must already have its parent.
pub(crate) fn stage_directory(source: &Path, target: &Path) -> Result<PathBuf> {
    ensure!(
        source.is_dir(),
        "source directory does not exist: {}",
        source.display()
    );
    let staged = sibling_path(target, "tmp")?;
    let copied = copy_tree(source, &staged);
    if copied.is_err() {
        let _ = remove_path(&staged);
    }
    copied.map(|()| staged)
}

fn copy_tree(source: &Path, staged: &Path) -> Result<()> {
    std::fs::create_dir(staged)
        .with_context(|| format!("failed to create directory {}", staged.display()))?;
    for file in regular_files(source)? {
        let relative = file
            .strip_prefix(source)
            .with_context(|| format!("failed to strip prefix {}", source.display()))?;
        copy_file(&file, &staged.join(relative))?;
    }
    Ok(())
}

/// Writes `content` into a fresh sibling of `target`, which must already have its parent.
pub(crate) fn stage_file(target: &Path, content: &[u8]) -> Result<PathBuf> {
    let (path, mut file) = create_temporary_file(target)?;
    let written = file
        .write_all(content)
        .with_context(|| format!("failed to write temporary file for {}", target.display()));
    if written.is_err() {
        let _ = remove_path(&path);
    }
    written.map(|()| path)
}

/// Renames `staged` over `target`, returning the backup of what was there.
pub(crate) fn swap_in(staged: &Path, target: &Path) -> Result<Option<PathBuf>> {
    let backup = back_up(target)?;
    let renamed = std::fs::rename(staged, target)
        .with_context(|| format!("failed to replace {}", target.display()));
    if renamed.is_err() {
        restore(target, backup.as_deref())?;
    }
    renamed.map(|()| backup)
}

/// Backs up a regular file with a hard link so it stays in place until the rename replaces it.
fn back_up(target: &Path) -> Result<Option<PathBuf>> {
    let is_file = std::fs::symlink_metadata(target).is_ok_and(|metadata| metadata.is_file());
    if !is_file {
        return move_aside(target);
    }
    let backup = sibling_path(target, "bak")?;
    std::fs::hard_link(target, &backup)
        .or_else(|_| std::fs::copy(target, &backup).map(drop))
        .with_context(|| format!("failed to back up {}", target.display()))?;
    Ok(Some(backup))
}

/// Creates the missing parent directories of `target` and returns them, outermost first.
pub(crate) fn create_missing_parents(target: &Path) -> Result<Vec<PathBuf>> {
    let Some(parent) = target.parent() else {
        return Ok(Vec::new());
    };
    let mut missing: Vec<PathBuf> = parent
        .ancestors()
        .take_while(|ancestor| !ancestor.exists())
        .map(Path::to_path_buf)
        .collect();
    missing.reverse();
    create_parent(target)?;
    Ok(missing)
}

/// Removes directories `create_missing_parents` made, innermost first, if they are still empty.
pub(crate) fn remove_created_parents(created: &[PathBuf]) {
    for directory in created.iter().rev() {
        let _ = std::fs::remove_dir(directory);
    }
}

fn create_parent(target: &Path) -> Result<()> {
    let Some(parent) = target.parent() else {
        return Ok(());
    };
    std::fs::create_dir_all(parent)
        .with_context(|| format!("failed to create directory {}", parent.display()))
}

/// Renames an existing `target` to a sibling backup, or returns `None` when nothing exists.
pub(crate) fn move_aside(target: &Path) -> Result<Option<PathBuf>> {
    if std::fs::symlink_metadata(target).is_err() {
        return Ok(None);
    }
    let backup = sibling_path(target, "bak")?;
    std::fs::rename(target, &backup)
        .with_context(|| format!("failed to move {} aside", target.display()))?;
    Ok(Some(backup))
}

/// Puts `backup` back at `target`, discarding whatever was published there meanwhile.
pub(crate) fn restore(target: &Path, backup: Option<&Path>) -> Result<()> {
    let Some(backup) = backup else {
        return remove_path(target);
    };
    if std::fs::symlink_metadata(target).is_ok_and(|metadata| metadata.is_dir()) {
        remove_path(target)?;
    }
    std::fs::rename(backup, target)
        .with_context(|| format!("failed to restore {}", target.display()))
}

pub(crate) fn discard(backup: &Path) -> Result<()> {
    remove_path(backup)
}

fn remove_path(path: &Path) -> Result<()> {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return Ok(());
    };
    let removal = if metadata.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    };
    removal.with_context(|| format!("failed to remove {}", path.display()))
}

fn sibling_path(target: &Path, label: &str) -> Result<PathBuf> {
    static NEXT_ID: AtomicU64 = AtomicU64::new(0);
    let name = target.file_name().context("output path must name a file")?;
    let candidate = || {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let mut sibling_name = std::ffi::OsString::from(".");
        sibling_name.push(name);
        sibling_name.push(format!(".{}.{id}.{label}", std::process::id()));
        target.with_file_name(sibling_name)
    };
    std::iter::repeat_with(candidate)
        .find(|path| std::fs::symlink_metadata(path).is_err())
        .context("no free sibling path")
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

#[cfg(test)]
mod directory_tests {
    use super::*;

    fn root(name: &str) -> Result<PathBuf> {
        let root =
            std::env::temp_dir().join(format!("agent-harness-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root)?;
        Ok(root)
    }

    #[test]
    fn copy_dir_rejects_a_missing_source_and_keeps_the_target() -> Result<()> {
        let root = root("copy-dir-missing")?;
        std::fs::create_dir_all(root.join("target"))?;
        std::fs::write(root.join("target/kept"), "kept\n")?;

        assert!(copy_dir(&root.join("absent"), &root.join("target")).is_err());

        assert_eq!(std::fs::read_to_string(root.join("target/kept"))?, "kept\n");
        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn copy_dir_replaces_the_target_and_leaves_no_sibling_behind() -> Result<()> {
        let root = root("copy-dir-replace")?;
        std::fs::create_dir_all(root.join("source"))?;
        std::fs::write(root.join("source/new"), "new\n")?;
        std::fs::create_dir_all(root.join("target"))?;
        std::fs::write(root.join("target/old"), "old\n")?;

        copy_dir(&root.join("source"), &root.join("target"))?;

        assert!(!root.join("target/old").exists());
        assert_eq!(std::fs::read_to_string(root.join("target/new"))?, "new\n");
        assert_eq!(std::fs::read_dir(&root)?.count(), 2);
        std::fs::remove_dir_all(root)?;
        Ok(())
    }
}
