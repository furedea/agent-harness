use std::{
    cell::RefCell,
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};

use super::*;
use crate::{filesystem::FileSystem, install_executor::execute};

#[derive(Default)]
struct MemoryFileSystem(RefCell<BTreeMap<PathBuf, Vec<u8>>>);

impl FileSystem for MemoryFileSystem {
    fn copy_file(&self, source: &Path, target: &Path) -> Result<()> {
        self.write_file(target, &self.read_file(source)?)
    }

    fn replace_directory(&self, source: &Path, target: &Path) -> Result<()> {
        let files = relocated_files(&self.0.borrow(), source, target);
        self.0
            .borrow_mut()
            .retain(|path, _| !path.starts_with(target));
        self.0.borrow_mut().extend(files);
        Ok(())
    }

    fn write_file(&self, target: &Path, content: &[u8]) -> Result<()> {
        self.0
            .borrow_mut()
            .insert(target.to_owned(), content.to_vec());
        Ok(())
    }

    fn read_file(&self, source: &Path) -> Result<Vec<u8>> {
        self.0.borrow().get(source).cloned().context("missing file")
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
    fs.write_file(
        Path::new("staging/.hermes/plugins/agent-harness-hooks/plugin.yaml"),
        b"plugin\n",
    )?;
    fs.write_file(
        Path::new("home/.hermes/plugins/agent-harness-hooks/stale"),
        b"stale",
    )?;
    fs.write_file(Path::new("home/.hermes/config.yaml"), b"user\n")?;
    let before = fs.0.borrow().clone();
    let plan = plan_install(&rendered, &installed);
    assert_eq!(*fs.0.borrow(), before);
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
    assert!(fs.0.borrow().is_empty());
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
    assert_eq!(fs.0.borrow().len(), 6);
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
