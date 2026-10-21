use std::fs::DirEntry;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::{fs_ops, generation::io};

const SUPPORTED_SPEC_VERSION: u64 = 1;
const BUNDLE_VERSION: u64 = 1;
const PROVIDER_DIRS: [&str; 5] = [".claude", ".codex", ".devin", ".hermes", ".pi"];
const SUPPORTED_FILES: [&str; 6] = [
    ".claude/settings.json",
    ".codex/hooks.json",
    ".codex/config.toml",
    ".devin/hooks.v1.json",
    ".hermes/hooks.json",
    ".pi/hooks.json",
];
const SUPPORTED_HOOK_DIRS: [&str; 5] = [
    ".claude/hooks",
    ".codex/hooks",
    ".devin/hooks",
    ".hermes/hooks",
    ".pi/hooks",
];

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
        ensure!(
            self.version == SUPPORTED_SPEC_VERSION,
            "unsupported hook bundle spec version in {}: {}",
            path.display(),
            self.version,
        );
        ensure!(
            !self.installers.is_empty(),
            "hook bundle spec must contain at least one installer"
        );
        self.installers.iter().try_for_each(Installer::validate)?;
        self.command_replacements
            .iter()
            .try_for_each(CommandReplacement::validate)
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Installer {
    executable: PathBuf,
    #[serde(default)]
    arguments: Vec<String>,
}

impl Installer {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.executable.is_absolute(),
            "hook installer executable must be an absolute path: {}",
            self.executable.display(),
        );
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CommandReplacement {
    from: String,
    to: String,
}

impl CommandReplacement {
    fn validate(&self) -> Result<()> {
        ensure!(
            !self.from.is_empty() && !self.to.is_empty(),
            "hook command replacements must not contain empty values"
        );
        Ok(())
    }
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
        create_provider_dirs(&path)?;
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

fn create_provider_dirs(home: &Path) -> Result<()> {
    PROVIDER_DIRS
        .iter()
        .try_for_each(|provider| create_provider_dir(home, provider))
}

fn create_provider_dir(home: &Path, provider: &str) -> Result<()> {
    std::fs::create_dir(home.join(provider))
        .with_context(|| format!("failed to create temporary provider directory {provider}"))
}

fn run_installers(installers: &[Installer], staging: &Path) -> Result<()> {
    installers
        .iter()
        .try_for_each(|installer| run_installer(installer, staging))
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

/// Rewrites staged hook commands so they point at the bundle output.
struct CommandNormalization<'a> {
    staging: &'a Path,
    output: &'a Path,
    replacements: &'a [CommandReplacement],
}

fn capture_supported_files(
    staging: &Path,
    output: &Path,
    replacements: &[CommandReplacement],
) -> Result<()> {
    let normalization = CommandNormalization {
        staging,
        output,
        replacements,
    };
    let mut captured = false;
    for relative in SUPPORTED_FILES {
        captured |= capture_file(relative, &normalization)?;
    }
    for relative in SUPPORTED_HOOK_DIRS {
        captured |= capture_dir(relative, staging, output)?;
    }
    captured |= copy_codex_scripts(staging, output)?;
    ensure!(
        captured,
        "hook installers did not generate any supported artifacts"
    );
    Ok(())
}

fn capture_file(relative: &str, normalization: &CommandNormalization) -> Result<bool> {
    let source = normalization.staging.join(relative);
    if !source.is_file() {
        return Ok(false);
    }
    let target = normalization.output.join(relative);
    if source.extension().and_then(|extension| extension.to_str()) == Some("json") {
        copy_normalized_json(&source, &target, normalization)?;
    } else {
        fs_ops::copy_file(&source, &target)?;
    }
    Ok(true)
}

fn capture_dir(relative: &str, staging: &Path, output: &Path) -> Result<bool> {
    let source = staging.join(relative);
    if !source.is_dir() {
        return Ok(false);
    }
    fs_ops::copy_dir(&source, &output.join(relative))?;
    Ok(true)
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
        copied |= copy_shell_script(&entry, output)?;
    }
    Ok(copied)
}

fn copy_shell_script(entry: &DirEntry, output: &Path) -> Result<bool> {
    let path = entry.path();
    let is_file = entry
        .file_type()
        .with_context(|| format!("failed to inspect {}", path.display()))?
        .is_file();
    let is_shell = path.extension().and_then(|extension| extension.to_str()) == Some("sh");
    if !(is_file && is_shell) {
        return Ok(false);
    }
    fs_ops::copy_file(&path, &output.join(".codex").join(entry.file_name()))?;
    Ok(true)
}

fn copy_normalized_json(
    source: &Path,
    target: &Path,
    normalization: &CommandNormalization,
) -> Result<()> {
    let mut value: Value = serde_json::from_str(
        &std::fs::read_to_string(source)
            .with_context(|| format!("failed to read {}", source.display()))?,
    )
    .with_context(|| format!("failed to parse {}", source.display()))?;
    normalize_commands(&mut value, normalization);
    io::write_json(target, &value)
}

fn normalize_commands(value: &mut Value, normalization: &CommandNormalization) {
    match value {
        Value::Array(items) => items
            .iter_mut()
            .for_each(|item| normalize_commands(item, normalization)),
        Value::Object(object) => normalize_object_commands(object, normalization),
        _ => {}
    }
}

fn normalize_object_commands(
    object: &mut Map<String, Value>,
    normalization: &CommandNormalization,
) {
    if let Some(Value::String(command)) = object.get_mut("command") {
        *command = normalize_command(command, normalization);
    }
    object
        .values_mut()
        .for_each(|item| normalize_commands(item, normalization));
}

fn normalize_command(command: &str, normalization: &CommandNormalization) -> String {
    let command = command.replace(
        normalization.staging.to_string_lossy().as_ref(),
        normalization.output.to_string_lossy().as_ref(),
    );
    normalization
        .replacements
        .iter()
        .fold(command, |command, replacement| {
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
