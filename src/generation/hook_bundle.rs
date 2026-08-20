use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde_json::Value;

use crate::{fs_ops, generation::io};

const SUPPORTED_SPEC_VERSION: u64 = 1;
const BUNDLE_VERSION: u64 = 1;

pub(crate) fn generate(spec_path: &Path, output: &Path) -> Result<()> {
    let spec = read_spec(spec_path)?;
    spec.validate(spec_path)?;
    if output.exists() {
        bail!("hook bundle output already exists: {}", output.display());
    }

    let staging = TemporaryHome::create()?;
    run_installers(&spec.installers, staging.path())?;
    capture_supported_files(staging.path(), output, &spec.command_replacements)?;
    write_manifest(output)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct HookBundleSpec {
    version: u64,
    installers: Vec<Installer>,
    #[serde(default)]
    command_replacements: Vec<CommandReplacement>,
}

impl HookBundleSpec {
    fn validate(&self, path: &Path) -> Result<()> {
        if self.version != SUPPORTED_SPEC_VERSION {
            bail!(
                "unsupported hook bundle spec version in {}: {}",
                path.display(),
                self.version,
            );
        }
        if self.installers.is_empty() {
            bail!("hook bundle spec must contain at least one installer");
        }
        for installer in &self.installers {
            if !installer.executable.is_absolute() {
                bail!(
                    "hook installer executable must be an absolute path: {}",
                    installer.executable.display(),
                );
            }
        }
        for replacement in &self.command_replacements {
            if replacement.from.is_empty() || replacement.to.is_empty() {
                bail!("hook command replacements must not contain empty values");
            }
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Installer {
    executable: PathBuf,
    #[serde(default)]
    arguments: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CommandReplacement {
    from: String,
    to: String,
}

struct TemporaryHome {
    path: PathBuf,
}

impl TemporaryHome {
    fn create() -> Result<Self> {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = std::env::temp_dir().join(format!(
            "agent-harness-hook-bundle-{}-{nanos}",
            std::process::id(),
        ));
        std::fs::create_dir(&path)
            .with_context(|| format!("failed to create temporary home {}", path.display()))?;
        for provider in [".claude", ".codex"] {
            std::fs::create_dir(path.join(provider)).with_context(|| {
                format!("failed to create temporary provider directory {provider}")
            })?;
        }
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TemporaryHome {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn run_installers(installers: &[Installer], staging: &Path) -> Result<()> {
    for installer in installers {
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
        if !result.status.success() {
            bail!(
                "hook installer {} failed: {}",
                installer.executable.display(),
                String::from_utf8_lossy(&result.stderr).trim(),
            );
        }
    }
    Ok(())
}

fn capture_supported_files(
    staging: &Path,
    output: &Path,
    replacements: &[CommandReplacement],
) -> Result<()> {
    let mut captured = false;
    for relative in [
        ".claude/settings.json",
        ".codex/hooks.json",
        ".codex/config.toml",
    ] {
        let source = staging.join(relative);
        if !source.is_file() {
            continue;
        }
        let target = output.join(relative);
        if source.extension().and_then(|extension| extension.to_str()) == Some("json") {
            copy_normalized_json(&source, &target, staging, output, replacements)?;
        } else {
            fs_ops::copy_file(&source, &target)?;
        }
        captured = true;
    }
    for relative in [".claude/hooks", ".codex/hooks"] {
        let source = staging.join(relative);
        if source.is_dir() {
            fs_ops::copy_dir(&source, &output.join(relative))?;
            captured = true;
        }
    }
    captured |= copy_codex_scripts(staging, output)?;
    if !captured {
        bail!("hook installers did not generate any supported artifacts");
    }
    Ok(())
}

fn copy_codex_scripts(staging: &Path, output: &Path) -> Result<bool> {
    let source = staging.join(".codex");
    if !source.is_dir() {
        return Ok(false);
    }

    let mut copied = false;
    for entry in std::fs::read_dir(&source)
        .with_context(|| format!("failed to read {}", source.display()))?
    {
        let entry =
            entry.with_context(|| format!("failed to read entry in {}", source.display()))?;
        let path = entry.path();
        if entry
            .file_type()
            .with_context(|| format!("failed to inspect {}", path.display()))?
            .is_file()
            && path.extension().and_then(|extension| extension.to_str()) == Some("sh")
        {
            fs_ops::copy_file(&path, &output.join(".codex").join(entry.file_name()))?;
            copied = true;
        }
    }
    Ok(copied)
}

fn copy_normalized_json(
    source: &Path,
    target: &Path,
    staging: &Path,
    output: &Path,
    replacements: &[CommandReplacement],
) -> Result<()> {
    let mut value: Value = serde_json::from_str(
        &std::fs::read_to_string(source)
            .with_context(|| format!("failed to read {}", source.display()))?,
    )
    .with_context(|| format!("failed to parse {}", source.display()))?;
    normalize_commands(&mut value, staging, output, replacements);
    io::write_json(target, &value)
}

fn normalize_commands(
    value: &mut Value,
    staging: &Path,
    output: &Path,
    replacements: &[CommandReplacement],
) {
    match value {
        Value::Array(items) => {
            for item in items {
                normalize_commands(item, staging, output, replacements);
            }
        }
        Value::Object(object) => {
            if let Some(Value::String(command)) = object.get_mut("command") {
                *command = normalize_command(command, staging, output, replacements);
            }
            for item in object.values_mut() {
                normalize_commands(item, staging, output, replacements);
            }
        }
        _ => {}
    }
}

fn normalize_command(
    command: &str,
    staging: &Path,
    output: &Path,
    replacements: &[CommandReplacement],
) -> String {
    let command = command.replace(
        staging.to_string_lossy().as_ref(),
        output.to_string_lossy().as_ref(),
    );
    replacements.iter().fold(command, |command, replacement| {
        command.replace(&replacement.from, &replacement.to)
    })
}

fn write_manifest(output: &Path) -> Result<()> {
    io::write_json(
        &output.join("hook_bundle.json"),
        &serde_json::json!({"version": BUNDLE_VERSION}),
    )
}

fn read_spec(path: &Path) -> Result<HookBundleSpec> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&content).with_context(|| format!("failed to parse {}", path.display()))
}
