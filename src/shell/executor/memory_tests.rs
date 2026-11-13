use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};

use super::*;
use crate::{
    install_plan::{plan_assets_for, plan_install},
    layout::InstalledLayout,
    shell::io::FileSystem,
};

/// Where a `MemoryFileSystem` fails: staging or swapping a path in, or restoring it.
#[derive(Clone, Copy, PartialEq)]
enum FailureMode {
    Stage,
    Swap,
    Restore,
}

/// In-memory files with an optional injected failure on one path.
#[derive(Default)]
struct MemoryFileSystem {
    files: RefCell<BTreeMap<PathBuf, Vec<u8>>>,
    failures: RefCell<Vec<(FailureMode, PathBuf)>>,
    directories: RefCell<BTreeSet<PathBuf>>,
}

impl MemoryFileSystem {
    fn fail(&self, mode: FailureMode, path: &str) {
        self.failures.borrow_mut().push((mode, PathBuf::from(path)));
    }

    fn check_failure(&self, mode: FailureMode, target: &Path) -> Result<()> {
        let injected = self.failures.borrow();
        anyhow::ensure!(
            !injected.contains(&(mode, target.to_owned())),
            "injected failure at {}",
            target.display()
        );
        Ok(())
    }

    fn write_file(&self, target: &Path, content: &[u8]) {
        self.files
            .borrow_mut()
            .insert(target.to_owned(), content.to_vec());
    }

    fn relocate(&self, from: &Path, to: &Path) {
        let moved = relocated_files(&self.files.borrow(), from, to);
        self.files
            .borrow_mut()
            .retain(|path, _| !path.starts_with(from));
        self.files.borrow_mut().extend(moved);
    }

    fn has_files_under(&self, path: &Path) -> bool {
        self.files
            .borrow()
            .keys()
            .any(|file| file.starts_with(path))
    }

    fn snapshot_under(&self, prefix: &str) -> BTreeMap<PathBuf, Vec<u8>> {
        self.files
            .borrow()
            .iter()
            .filter(|(path, _)| path.starts_with(prefix))
            .map(|(path, bytes)| (path.clone(), bytes.clone()))
            .collect()
    }
}

impl MemoryFileSystem {
    fn read_file(&self, source: &Path) -> Result<Vec<u8>> {
        self.files
            .borrow()
            .get(source)
            .cloned()
            .context("missing file")
    }
}

impl FileSystem for MemoryFileSystem {
    fn create_parents(&self, target: &Path, _mode: FileMode) -> Result<Vec<PathBuf>> {
        let mut directories = self.directories.borrow_mut();
        let mut missing: Vec<PathBuf> = target
            .ancestors()
            .skip(1)
            .filter(|ancestor| !ancestor.as_os_str().is_empty())
            .filter(|ancestor| !directories.contains(*ancestor))
            .map(Path::to_path_buf)
            .collect();
        missing.reverse();
        directories.extend(missing.iter().cloned());
        Ok(missing)
    }

    fn remove_created(&self, created: &[PathBuf]) {
        self.directories
            .borrow_mut()
            .retain(|directory| !created.contains(directory));
    }

    fn stage_directory(&self, source: &Path, target: &Path) -> Result<PathBuf> {
        self.check_failure(FailureMode::Stage, target)?;
        let staged = PathBuf::from("staged").join(target);
        let files = relocated_files(&self.files.borrow(), source, &staged);
        anyhow::ensure!(!files.is_empty(), "missing source directory");
        self.files.borrow_mut().extend(files);
        Ok(staged)
    }

    fn stage_file(&self, target: &Path, file: &NewFile<'_>) -> Result<PathBuf> {
        self.check_failure(FailureMode::Stage, target)?;
        let staged = PathBuf::from("staged").join(target);
        self.write_file(&staged, file.content);
        Ok(staged)
    }

    fn stage_copy(&self, source: &Path, target: &Path) -> Result<PathBuf> {
        self.check_failure(FailureMode::Stage, target)?;
        let staged = PathBuf::from("staged").join(target);
        self.write_file(&staged, &self.read_file(source)?);
        Ok(staged)
    }

    fn stage_tree(&self, target: &Path, contents: &[FileOp]) -> Result<PathBuf> {
        self.check_failure(FailureMode::Stage, target)?;
        let staged = PathBuf::from("staged").join(target);
        let writes = contents
            .iter()
            .filter_map(|operation| match operation {
                FileOp::WriteFile {
                    target, content, ..
                } => Some(
                    target
                        .strip_prefix(operation.target())
                        .map(|relative| (staged.join(relative), content)),
                ),
                _ => None,
            })
            .collect::<Result<Vec<_>, _>>()?;
        write_all(self, &writes);
        Ok(staged)
    }

    fn swap(&self, staged: &Path, target: &Path) -> Result<Option<PathBuf>> {
        self.check_failure(FailureMode::Swap, target)?;
        let backup = PathBuf::from("backup").join(target);
        let existed = self.has_files_under(target);
        self.relocate(target, &backup);
        self.relocate(staged, target);
        Ok(existed.then_some(backup))
    }

    fn restore(&self, target: &Path, backup: Option<&Path>) -> Result<()> {
        self.check_failure(FailureMode::Restore, target)?;
        self.files
            .borrow_mut()
            .retain(|path, _| !path.starts_with(target));
        relocate_backup(self, backup, target);
        Ok(())
    }

    fn discard(&self, path: &Path) -> Result<()> {
        self.files
            .borrow_mut()
            .retain(|file, _| !file.starts_with(path));
        Ok(())
    }
}

#[test]
fn install_replaces_managed_trees_and_preserves_user_files() -> Result<()> {
    let fs = MemoryFileSystem::default();
    let rendered = InstalledLayout::new(Path::new("staging"));
    let installed = InstalledLayout::new(Path::new("home"));
    for path in rendered.managed_files_for(crate::provider::spec::PROVIDERS) {
        fs.write_file(&path, b"generated\n");
    }
    for directory in rendered.managed_directories_for(crate::provider::spec::PROVIDERS) {
        fs.write_file(&directory.join("keep"), b"keep\n");
    }
    fs.write_file(
        Path::new("staging/.hermes/plugins/agent-harness-hooks/plugin.yaml"),
        b"plugin\n",
    );
    fs.write_file(
        Path::new("home/.hermes/plugins/agent-harness-hooks/stale"),
        b"stale",
    );
    fs.write_file(Path::new("home/.hermes/config.yaml"), b"user\n");
    let before = fs.files.borrow().clone();
    let plan = plan_install(&rendered, &installed);
    assert_eq!(*fs.files.borrow(), before);
    execute_on(&fs, &plan)?;
    assert_eq!(
        fs.read_file(Path::new("home/.hermes/config.yaml"))?,
        b"user\n"
    );
    assert_eq!(
        fs.read_file(Path::new("home/.pi/agent/extensions/hook_bridge.ts"))?,
        b"generated\n"
    );
    assert_eq!(
        fs.read_file(Path::new(
            "home/.hermes/plugins/agent-harness-hooks/plugin.yaml"
        ))?,
        b"plugin\n"
    );
    assert!(
        fs.read_file(Path::new("home/.hermes/plugins/agent-harness-hooks/stale"))
            .is_err()
    );
    Ok(())
}

#[test]
fn execution_stops_on_a_missing_rendered_file() {
    let fs = MemoryFileSystem::default();
    let plan = vec![
        FileOp::CopyFile {
            source: "missing".into(),
            target: "target".into(),
        },
        FileOp::CopyFile {
            source: "later".into(),
            target: "later-target".into(),
        },
    ];
    assert!(execute_on(&fs, &plan).is_err());
    assert!(fs.files.borrow().is_empty());
}

#[test]
fn asset_plan_installs_every_provider_at_its_output_relative_destination() -> Result<()> {
    let fs = MemoryFileSystem::default();
    execute_on(
        &fs,
        &plan_assets_for(Path::new("home"), crate::provider::spec::PROVIDERS),
    )?;
    for expected in [
        "home/.hermes/plugins/agent-harness-hooks/plugin.yaml",
        "home/.hermes/plugins/agent-harness-hooks/__init__.py",
        "home/.pi/agent/extensions/hook_bridge.ts",
        "home/.pi/agent/extensions/sandbox/file_operations.mjs",
        "home/.pi/agent/extensions/bash_sandbox.ts",
        "home/.pi/agent/extensions/sandbox_policy.json",
        "home/.dsh/bridge/package.json",
        "home/.dsh/bridge/cordis.patch.yml",
        "home/.dsh/bridge/hook_bridge.ts",
    ] {
        assert!(!fs.read_file(Path::new(expected))?.is_empty());
    }
    assert_eq!(fs.files.borrow().len(), 9);
    Ok(())
}

fn relocated_files(
    files: &BTreeMap<PathBuf, Vec<u8>>,
    source: &Path,
    target: &Path,
) -> Vec<(PathBuf, Vec<u8>)> {
    files
        .iter()
        .filter_map(|(path, bytes)| {
            path.strip_prefix(source)
                .ok()
                .map(|relative| (target.join(relative), bytes.clone()))
        })
        .collect()
}

fn write_all(fs: &MemoryFileSystem, writes: &[(PathBuf, &Vec<u8>)]) {
    for (path, content) in writes {
        fs.write_file(path, content);
    }
}

fn relocate_backup(fs: &MemoryFileSystem, backup: Option<&Path>, target: &Path) {
    if let Some(backup) = backup {
        fs.relocate(backup, target);
    }
}

fn installed_state() -> (MemoryFileSystem, Vec<FileOp>) {
    let fs = MemoryFileSystem::default();
    let rendered = InstalledLayout::new(Path::new("staging"));
    let installed = InstalledLayout::new(Path::new("home"));
    for path in rendered.managed_files_for(crate::provider::spec::PROVIDERS) {
        fs.write_file(&path, b"generated\n");
    }
    for directory in rendered.managed_directories_for(crate::provider::spec::PROVIDERS) {
        fs.write_file(&directory.join("keep"), b"keep\n");
    }
    fs.write_file(Path::new("home/.claude/hooks/old.sh"), b"old\n");
    fs.write_file(Path::new("home/.claude/CLAUDE.md"), b"old instructions\n");
    (fs, plan_install(&rendered, &installed))
}

#[test]
fn a_failure_midway_restores_every_installed_path() {
    let (fs, plan) = installed_state();
    fs.fail(FailureMode::Swap, "home/.codex/hooks.json");
    let before = fs.snapshot_under("home");

    assert!(execute_on(&fs, &plan).is_err());

    assert_eq!(fs.snapshot_under("home"), before);
    assert!(fs.snapshot_under("backup").is_empty());
    assert!(fs.snapshot_under("staged").is_empty());
    assert!(fs.directories.borrow().is_empty());
}

#[test]
fn a_failure_while_staging_leaves_every_live_path_untouched() {
    let (fs, plan) = installed_state();
    fs.fail(FailureMode::Stage, "home/.codex/hooks.json");
    let before = fs.snapshot_under("home");

    assert!(execute_on(&fs, &plan).is_err());

    assert_eq!(fs.snapshot_under("home"), before);
    assert!(fs.snapshot_under("staged").is_empty());
    assert!(fs.directories.borrow().is_empty());
}

#[test]
fn rollback_restores_the_remaining_paths_when_one_restore_fails() -> Result<()> {
    let (fs, plan) = installed_state();
    let first = plan.first().context("empty plan")?.target();
    let last = plan.last().context("empty plan")?.target();
    fs.fail(FailureMode::Swap, &last.to_string_lossy());
    fs.fail(FailureMode::Restore, &first.to_string_lossy());

    let error = execute_on(&fs, &plan).unwrap_err();

    assert!(format!("{error:#}").contains("rollback also failed"));
    assert!(fs.directories.borrow().is_empty());
    assert_eq!(
        fs.read_file(Path::new("home/.claude/CLAUDE.md"))?,
        b"old instructions\n"
    );
    assert_eq!(
        fs.read_file(Path::new("home/.claude/hooks/old.sh"))?,
        b"old\n"
    );
    Ok(())
}

#[test]
fn a_missing_source_directory_keeps_the_installed_directory() -> Result<()> {
    let fs = MemoryFileSystem::default();
    fs.write_file(Path::new("home/.claude/hooks/old.sh"), b"old\n");
    let plan = vec![FileOp::CopyTree {
        source: "staging/.claude/hooks".into(),
        target: "home/.claude/hooks".into(),
    }];

    assert!(execute_on(&fs, &plan).is_err());

    assert_eq!(
        fs.read_file(Path::new("home/.claude/hooks/old.sh"))?,
        b"old\n"
    );
    Ok(())
}

#[test]
fn injected_failures_name_the_failing_path() {
    let fs = MemoryFileSystem::default();
    fs.fail(FailureMode::Stage, "home/file");
    let plan = vec![FileOp::write("home/file", b"x".to_vec())];
    let error = execute_on(&fs, &plan).unwrap_err();
    assert!(format!("{error:#}").contains("injected failure at home/file"));
}
