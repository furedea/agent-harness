use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::{file_op::FileOp, provider::spec::PROVIDERS};

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
    /// Parses and validates the spec `content` read from `path`.
    pub(crate) fn parse(path: &Path, content: &str) -> Result<Self> {
        let spec: Self = serde_json::from_str(content)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        spec.validate(path)?;
        Ok(spec)
    }

    /// The files of a finished installer run in `staging` that the bundle `output` holds.
    pub(crate) fn capture_ops(&self, staging: &Staged, output: &Path) -> Result<Vec<FileOp>> {
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

/// What a finished installer run left in its isolated home, read by the loader.
#[derive(Debug)]
pub(crate) struct Staged {
    pub(crate) root: PathBuf,
    /// The supported hook and config files that exist, in the order they are captured.
    pub(crate) files: Vec<StagedFile>,
    /// The provider hook directories that exist.
    pub(crate) directories: Vec<StagedDirectory>,
    /// The shell scripts that sit directly in `.codex`.
    pub(crate) codex_scripts: Vec<PathBuf>,
}

#[derive(Debug)]
pub(crate) struct StagedFile {
    pub(crate) relative: &'static str,
    pub(crate) content: StagedContent,
}

#[derive(Debug)]
pub(crate) enum StagedContent {
    /// A JSON document, parsed.
    Json(Value),
    /// Any other file, which is copied as it is.
    Other,
}

#[derive(Debug)]
pub(crate) struct StagedDirectory {
    pub(crate) relative: &'static str,
}

/// The supported files of a bundle, relative to the home: each provider's external hook file and
/// the Codex config.
pub(crate) fn supported_files() -> impl Iterator<Item = &'static str> {
    PROVIDERS.iter().map(|spec| spec.hook.external_file).chain(
        crate::provider::spec::ProviderId::Codex
            .spec()
            .config_file(),
    )
}

/// Whether the file at `relative` is a JSON document.
pub(crate) fn is_json_file(relative: &str) -> bool {
    Path::new(relative)
        .extension()
        .and_then(|extension| extension.to_str())
        == Some("json")
}

fn capture_supported_files(
    staging: &Staged,
    output: &Path,
    replacements: &[CommandReplacement],
) -> Result<Vec<FileOp>> {
    let normalization = CommandNormalization {
        staging,
        output,
        replacements,
    };
    let mut operations = Vec::new();
    for file in &staging.files {
        operations.push(normalization.capture_file(file)?);
    }
    operations.extend(
        staging
            .directories
            .iter()
            .map(|directory| FileOp::CopyTree {
                source: staging.root.join(directory.relative),
                target: output.join(directory.relative),
            }),
    );
    operations.extend(codex_script_ops(staging, output));
    Ok(operations)
}

/// Rewrites staged hook commands so they point at the bundle output.
struct CommandNormalization<'a> {
    staging: &'a Staged,
    output: &'a Path,
    replacements: &'a [CommandReplacement],
}

impl CommandNormalization<'_> {
    fn capture_file(&self, file: &StagedFile) -> Result<FileOp> {
        let target = self.output.join(file.relative);
        match &file.content {
            StagedContent::Json(value) => self.normalized_json_op(value, target),
            StagedContent::Other => Ok(FileOp::CopyFile {
                source: self.staging.root.join(file.relative),
                target,
            }),
        }
    }

    fn normalized_json_op(&self, value: &Value, target: PathBuf) -> Result<FileOp> {
        let mut value = value.clone();
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
            self.staging.root.to_string_lossy().as_ref(),
            self.output.to_string_lossy().as_ref(),
        );
        self.replacements
            .iter()
            .fold(command, |command, rule| rule.apply(&command))
    }
}

fn rewrite_command(object: &mut Map<String, Value>, rewrite: impl FnOnce(&str) -> String) {
    if let Some(Value::String(command)) = object.get_mut("command") {
        *command = rewrite(command);
    }
}

fn codex_script_ops(staging: &Staged, output: &Path) -> Vec<FileOp> {
    staging
        .codex_scripts
        .iter()
        .filter_map(|script| {
            Some(FileOp::CopyFile {
                target: output.join(".codex").join(script.file_name()?),
                source: script.clone(),
            })
        })
        .collect()
}

fn manifest_op(output: &Path) -> Result<FileOp> {
    FileOp::write_json(
        output.join("hook_bundle.json"),
        &serde_json::json!({"version": BUNDLE_VERSION}),
    )
}
