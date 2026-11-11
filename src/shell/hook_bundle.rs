#![expect(
    clippy::disallowed_methods,
    reason = "running the installers is the point of this command, and so is checking its output"
)]

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail, ensure};

use super::{TemporaryDirectory, execute, load_snapshot};
use crate::{
    file_op::FileOp,
    generation::hook_bundle::{HookBundleSpec, Installer},
    provider::spec::PROVIDERS,
    snapshot::{SnapshotRequest, Tree},
};

/// Runs a spec's installers in an isolated home and captures what they wrote as a bundle.
pub(crate) fn generate(spec_path: &Path, output: &Path) -> Result<()> {
    let spec = read_spec(spec_path)?;
    if output.exists() {
        bail!("hook bundle output already exists: {}", output.display());
    }

    let home = TemporaryDirectory::create()?;
    execute(&provider_directories(home.path()))?;
    spec.installers
        .iter()
        .try_for_each(|installer| run_installer(installer, home.path()))?;
    let staged = load_snapshot(&HookBundleSpec::capture_request(home.path()))?;
    execute(&spec.capture_ops(Tree::new(home.path(), &staged), output)?)
}

fn read_spec(path: &Path) -> Result<HookBundleSpec> {
    let request = SnapshotRequest {
        documents: vec![path.to_path_buf()],
        ..SnapshotRequest::default()
    };
    let files = load_snapshot(&request)?;
    let content = files
        .read_text(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    HookBundleSpec::parse(path, content)
}

fn provider_directories(home: &Path) -> Vec<FileOp> {
    PROVIDERS
        .iter()
        .map(|spec| FileOp::ReplaceTree {
            target: home.join(spec.home_dir),
            contents: Vec::new(),
        })
        .collect()
}

fn run_installer(installer: &Installer, staging: &Path) -> Result<()> {
    let result = Command::new(&installer.executable)
        .args(&installer.arguments)
        .current_dir(staging)
        .env("HOME", staging)
        .env("XDG_CACHE_HOME", staging.join(".cache"))
        .env("XDG_CONFIG_HOME", staging.join(".config"))
        .env("XDG_DATA_HOME", staging.join(".local/share"))
        .env("XDG_STATE_HOME", staging.join(".local/state"))
        .env_remove("CLAUDE_CONFIG_DIR")
        .env_remove("CODEX_HOME")
        .output()
        .with_context(|| format!("failed to execute {}", installer.executable.display()))?;
    ensure!(
        result.status.success(),
        "hook installer {} failed: {}",
        installer.executable.display(),
        String::from_utf8_lossy(&result.stderr).trim(),
    );
    Ok(())
}
