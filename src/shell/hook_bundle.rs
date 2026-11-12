#![expect(
    clippy::disallowed_methods,
    reason = "running the installers is the point of this command, and so is checking its output"
)]

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail, ensure};

use super::{TemporaryDirectory, execute, load::shell_scripts};
use crate::{
    file_op::FileOp,
    generation::hook_bundle::{
        HookBundleSpec, Installer, Staged, StagedContent, StagedDirectory, StagedFile,
        StagedScript, is_json_file, script_directories, supported_files,
    },
    provider::spec::PROVIDERS,
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
    let staged = load_staged(home.path())?;
    execute(&spec.capture_ops(&staged, output)?)
}

fn read_spec(path: &Path) -> Result<HookBundleSpec> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    HookBundleSpec::parse(path, &content)
}

/// What the installers left in `home`: the supported files, the hook directories, and the
/// top-level scripts, with every JSON document parsed.
fn load_staged(home: &Path) -> Result<Staged> {
    let files = supported_files()
        .filter(|relative| home.join(relative).is_file())
        .map(|relative| staged_file(home, relative))
        .collect::<Result<Vec<_>>>()?;
    let directories = PROVIDERS
        .iter()
        .map(|spec| spec.hook.scripts_dir)
        .filter(|relative| home.join(relative).is_dir())
        .map(|relative| StagedDirectory { relative })
        .collect();
    Ok(Staged {
        root: home.to_path_buf(),
        files,
        directories,
        scripts: staged_scripts(home)?,
    })
}

fn staged_scripts(home: &Path) -> Result<Vec<StagedScript>> {
    let mut scripts = Vec::new();
    for directory in script_directories() {
        scripts.extend(
            shell_scripts(&home.join(directory))?
                .into_iter()
                .map(|path| StagedScript { directory, path }),
        );
    }
    Ok(scripts)
}

fn staged_file(home: &Path, relative: &'static str) -> Result<StagedFile> {
    Ok(StagedFile {
        relative,
        content: staged_content(relative, &home.join(relative))?,
    })
}

fn staged_content(relative: &str, path: &Path) -> Result<StagedContent> {
    if !is_json_file(relative) {
        return Ok(StagedContent::Other);
    }
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let value = serde_json::from_str(&text)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    Ok(StagedContent::Json(value))
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
