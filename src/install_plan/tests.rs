use std::{
    cell::RefCell,
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};

use super::*;
use crate::{filesystem::FileSystem, install_executor::execute};

/// In-memory files with an optional injected failure on publishing to one path.
#[derive(Default)]
struct MemoryFileSystem {
    files: RefCell<BTreeMap<PathBuf, Vec<u8>>>,
    fail_publishing_to: RefCell<Option<PathBuf>>,
}

impl MemoryFileSystem {
    fn failing_at(path: &str) -> Self {
        Self {
            fail_publishing_to: RefCell::new(Some(PathBuf::from(path))),
            ..Self::default()
        }
    }

    fn check_failure(&self, target: &Path) -> Result<()> {
        let injected = self.fail_publishing_to.borrow();
        anyhow::ensure!(
            injected.as_deref() != Some(target),
            "injected failure at {}",
            target.display()
        );
        Ok(())
    }

    fn relocate(&self, from: &Path, to: &Path) {
        let moved = relocated_files(&self.files.borrow(), from, to);
        self.files
            .borrow_mut()
            .retain(|path, _| !path.starts_with(from));
        self.files.borrow_mut().extend(moved);
    }
}

impl FileSystem for MemoryFileSystem {
    fn copy_file(&self, source: &Path, target: &Path) -> Result<()> {
        self.write_file(target, &self.read_file(source)?)
    }

    fn replace_directory(&self, source: &Path, target: &Path) -> Result<()> {
        self.check_failure(target)?;
        let files = relocated_files(&self.files.borrow(), source, target);
        anyhow::ensure!(!files.is_empty(), "missing source directory");
        self.files
            .borrow_mut()
            .retain(|path, _| !path.starts_with(target));
        self.files.borrow_mut().extend(files);
        Ok(())
    }

    fn write_file(&self, target: &Path, content: &[u8]) -> Result<()> {
        self.check_failure(target)?;
        self.files
            .borrow_mut()
            .insert(target.to_owned(), content.to_vec());
        Ok(())
    }

    fn read_file(&self, source: &Path) -> Result<Vec<u8>> {
        self.files
            .borrow()
            .get(source)
            .cloned()
            .context("missing file")
    }

    fn move_aside(&self, target: &Path) -> Result<Option<PathBuf>> {
        let backup = PathBuf::from("backup").join(target);
        let exists = self
            .files
            .borrow()
            .keys()
            .any(|path| path.starts_with(target));
        if exists {
            self.relocate(target, &backup);
        }
        Ok(exists.then_some(backup))
    }

    fn restore(&self, target: &Path, backup: Option<&Path>) -> Result<()> {
        self.files
            .borrow_mut()
            .retain(|path, _| !path.starts_with(target));
        if let Some(backup) = backup {
            self.relocate(backup, target);
        }
        Ok(())
    }

    fn discard(&self, backup: &Path) -> Result<()> {
        self.files
            .borrow_mut()
            .retain(|path, _| !path.starts_with(backup));
        Ok(())
    }
}

#[test]
fn install_replaces_managed_trees_and_preserves_user_files() -> Result<()> {
    let fs = MemoryFileSystem::default();
    let rendered = InstalledLayout::new(Path::new("staging"));
    let installed = InstalledLayout::new(Path::new("home"));
    for path in rendered.managed_files_for(crate::provider::spec::PROVIDERS) {
        fs.write_file(&path, b"generated\n")?;
    }
    for directory in rendered.managed_directories_for(crate::provider::spec::PROVIDERS) {
        fs.write_file(&directory.join("keep"), b"keep\n")?;
    }
    fs.write_file(
        Path::new("staging/.hermes/plugins/agent-harness-hooks/plugin.yaml"),
        b"plugin\n",
    )?;
    fs.write_file(
        Path::new("home/.hermes/plugins/agent-harness-hooks/stale"),
        b"stale",
    )?;
    fs.write_file(Path::new("home/.hermes/config.yaml"), b"user\n")?;
    let before = fs.files.borrow().clone();
    let plan = plan_install(&rendered, &installed);
    assert_eq!(*fs.files.borrow(), before);
    execute(&fs, &plan)?;
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
        FileOp::InstallFile {
            source: "missing".into(),
            target: "target".into(),
        },
        FileOp::InstallFile {
            source: "later".into(),
            target: "later-target".into(),
        },
    ];
    assert!(execute(&fs, &plan).is_err());
    assert!(fs.files.borrow().is_empty());
}

#[test]
fn asset_plan_installs_every_provider_at_its_output_relative_destination() -> Result<()> {
    let fs = MemoryFileSystem::default();
    execute(
        &fs,
        &plan_assets_for(Path::new("home"), crate::provider::spec::PROVIDERS),
    )?;
    for expected in [
        "home/.hermes/plugins/agent-harness-hooks/plugin.yaml",
        "home/.hermes/plugins/agent-harness-hooks/__init__.py",
        "home/.pi/agent/extensions/hook_bridge.ts",
        "home/.dsh/bridge/package.json",
        "home/.dsh/bridge/cordis.patch.yml",
        "home/.dsh/bridge/hook_bridge.ts",
    ] {
        assert!(!fs.read_file(Path::new(expected))?.is_empty());
    }
    assert_eq!(fs.files.borrow().len(), 6);
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

fn installed_state() -> Result<(MemoryFileSystem, Vec<FileOp>)> {
    let fs = MemoryFileSystem::default();
    let rendered = InstalledLayout::new(Path::new("staging"));
    let installed = InstalledLayout::new(Path::new("home"));
    for path in rendered.managed_files_for(crate::provider::spec::PROVIDERS) {
        fs.write_file(&path, b"generated\n")?;
    }
    fs.write_file(Path::new("staging/.claude/hooks/new.sh"), b"new\n")?;
    fs.write_file(Path::new("home/.claude/hooks/old.sh"), b"old\n")?;
    fs.write_file(Path::new("home/.claude/CLAUDE.md"), b"old instructions\n")?;
    Ok((fs, plan_install(&rendered, &installed)))
}

#[test]
fn a_failure_midway_restores_every_installed_path() -> Result<()> {
    let (fs, plan) = installed_state()?;
    *fs.fail_publishing_to.borrow_mut() = Some(PathBuf::from("home/.codex/hooks.json"));
    let before = fs.files.borrow().clone();
    let before: BTreeMap<_, _> = before
        .into_iter()
        .filter(|(path, _)| path.starts_with("home"))
        .collect();

    assert!(execute(&fs, &plan).is_err());

    let after: BTreeMap<_, _> = fs
        .files
        .borrow()
        .iter()
        .filter(|(path, _)| path.starts_with("home"))
        .map(|(path, bytes)| (path.clone(), bytes.clone()))
        .collect();
    assert_eq!(after, before);
    assert!(
        !fs.files
            .borrow()
            .keys()
            .any(|path| path.starts_with("backup"))
    );
    Ok(())
}

#[test]
fn a_missing_source_directory_keeps_the_installed_directory() -> Result<()> {
    let fs = MemoryFileSystem::default();
    fs.write_file(Path::new("home/.claude/hooks/old.sh"), b"old\n")?;
    let plan = vec![FileOp::ReplaceDirectory {
        source: "staging/.claude/hooks".into(),
        target: "home/.claude/hooks".into(),
    }];

    assert!(execute(&fs, &plan).is_err());

    assert_eq!(
        fs.read_file(Path::new("home/.claude/hooks/old.sh"))?,
        b"old\n"
    );
    Ok(())
}

#[test]
fn injected_failures_name_the_failing_path() {
    let fs = MemoryFileSystem::failing_at("home/file");
    let plan = vec![FileOp::WriteFile {
        target: "home/file".into(),
        content: b"x".to_vec(),
    }];
    let error = execute(&fs, &plan).unwrap_err();
    assert!(format!("{error:#}").contains("injected failure at home/file"));
}
