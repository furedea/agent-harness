use std::fs::DirEntry;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::{
    file_op::FileOp,
    provider::spec::{PROVIDERS, ProviderId},
};

const SUPPORTED_SPEC_VERSION: u64 = 1;
const BUNDLE_VERSION: u64 = 1;
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HookBundleSpec {
    version: u64,
    pub(crate) installers: Vec<Installer>,
    #[serde(default)]
    command_replacements: Vec<CommandReplacement>,
}

impl HookBundleSpec {
    /// Reads and validates the spec at `path`.
    pub(crate) fn read(path: &Path) -> Result<Self> {
        let spec = read_spec(path)?;
        spec.validate(path)?;
        Ok(spec)
    }

    /// The files of a finished installer run in `staging` that the bundle `output` holds.
    pub(crate) fn capture_ops(&self, staging: &Path, output: &Path) -> Result<Vec<FileOp>> {
        let mut operations = capture_supported_files(staging, output, &self.command_replacements)?;
        ensure!(
            !operations.is_empty(),
            "hook installers did not generate any supported artifacts"
        );
        operations.push(manifest_op(output)?);
        Ok(operations)
    }

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
pub(crate) struct Installer {
    pub(crate) executable: PathBuf,
    #[serde(default)]
    pub(crate) arguments: Vec<String>,
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
    fn apply(&self, command: &str) -> String {
        command.replace(&self.from, &self.to)
    }

    fn validate(&self) -> Result<()> {
        ensure!(
            !self.from.is_empty() && !self.to.is_empty(),
            "hook command replacements must not contain empty values"
        );
        Ok(())
    }
}

fn capture_supported_files(
    staging: &Path,
    output: &Path,
    replacements: &[CommandReplacement],
) -> Result<Vec<FileOp>> {
    let normalization = CommandNormalization {
        staging,
        output,
        replacements,
    };
    let mut operations = Vec::new();
    for relative in PROVIDERS
        .iter()
        .map(|spec| spec.hook.external_file)
        .chain(ProviderId::Codex.spec().config_file())
    {
        operations.extend(normalization.capture_file(relative)?);
    }
    for relative in PROVIDERS.iter().map(|spec| spec.hook.scripts_dir) {
        operations.extend(capture_dir(relative, staging, output));
    }
    operations.extend(codex_script_ops(staging, output)?);
    Ok(operations)
}

/// Rewrites staged hook commands so they point at the bundle output.
struct CommandNormalization<'a> {
    staging: &'a Path,
    output: &'a Path,
    replacements: &'a [CommandReplacement],
}

impl CommandNormalization<'_> {
    fn capture_file(&self, relative: &str) -> Result<Option<FileOp>> {
        let source = self.staging.join(relative);
        let target = self.output.join(relative);
        match (source.is_file(), is_json(&source)) {
            (false, _) => Ok(None),
            (true, true) => self.normalized_json_op(&source, &target).map(Some),
            (true, false) => Ok(Some(FileOp::CopyFile { source, target })),
        }
    }

    fn normalized_json_op(&self, source: &Path, target: &Path) -> Result<FileOp> {
        let mut value: Value = serde_json::from_str(
            &std::fs::read_to_string(source)
                .with_context(|| format!("failed to read {}", source.display()))?,
        )
        .with_context(|| format!("failed to parse {}", source.display()))?;
        self.normalize_commands(&mut value);
        FileOp::write_json(target, &value)
    }

    fn normalize_commands(&self, value: &mut Value) {
        match value {
            Value::Array(items) => items
                .iter_mut()
                .for_each(|item| self.normalize_commands(item)),
            Value::Object(object) => self.normalize_object_commands(object),
            _ => (),
        }
    }

    fn normalize_object_commands(&self, object: &mut Map<String, Value>) {
        rewrite_command(object, |command| self.normalize_command(command));
        object
            .values_mut()
            .for_each(|item| self.normalize_commands(item));
    }

    fn normalize_command(&self, command: &str) -> String {
        let command = command.replace(
            self.staging.to_string_lossy().as_ref(),
            self.output.to_string_lossy().as_ref(),
        );
        self.replacements
            .iter()
            .fold(command, |command, rule| rule.apply(&command))
    }
}

fn is_json(path: &Path) -> bool {
    path.extension().and_then(|extension| extension.to_str()) == Some("json")
}

fn rewrite_command(object: &mut Map<String, Value>, rewrite: impl FnOnce(&str) -> String) {
    if let Some(Value::String(command)) = object.get_mut("command") {
        *command = rewrite(command);
    }
}

fn capture_dir(relative: &str, staging: &Path, output: &Path) -> Option<FileOp> {
    let source = staging.join(relative);
    source.is_dir().then(|| FileOp::CopyTree {
        source,
        target: output.join(relative),
    })
}

fn codex_script_ops(staging: &Path, output: &Path) -> Result<Vec<FileOp>> {
    let source = staging.join(".codex");
    if !source.is_dir() {
        return Ok(Vec::new());
    }

    let mut operations = Vec::new();
    for entry in std::fs::read_dir(&source)
        .with_context(|| format!("failed to read {}", source.display()))?
    {
        let entry =
            entry.with_context(|| format!("failed to read entry in {}", source.display()))?;
        operations.extend(shell_script_op(&entry, output)?);
    }
    Ok(operations)
}

fn shell_script_op(entry: &DirEntry, output: &Path) -> Result<Option<FileOp>> {
    let path = entry.path();
    let is_file = entry
        .file_type()
        .with_context(|| format!("failed to inspect {}", path.display()))?
        .is_file();
    let is_shell = path.extension().and_then(|extension| extension.to_str()) == Some("sh");
    Ok((is_file && is_shell).then(|| FileOp::CopyFile {
        target: output.join(".codex").join(entry.file_name()),
        source: path,
    }))
}

fn manifest_op(output: &Path) -> Result<FileOp> {
    FileOp::write_json(
        output.join("hook_bundle.json"),
        &serde_json::json!({"version": BUNDLE_VERSION}),
    )
}

fn read_spec(path: &Path) -> Result<HookBundleSpec> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&content).with_context(|| format!("failed to parse {}", path.display()))
}
