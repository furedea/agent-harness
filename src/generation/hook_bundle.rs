use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::{
    file_op::FileOp,
    provider::spec::{PROVIDERS, ProviderId},
    snapshot::{Child, SnapshotRequest, Tree},
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
    /// Parses and validates the spec `content` read from `path`.
    pub(crate) fn parse(path: &Path, content: &str) -> Result<Self> {
        let spec: Self = serde_json::from_str(content)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        spec.validate(path)?;
        Ok(spec)
    }

    /// What `capture_ops` reads from `staging`, for the loader.
    pub(crate) fn capture_request(staging: &Path) -> SnapshotRequest {
        let codex = ProviderId::Codex.spec();
        SnapshotRequest {
            trees: PROVIDERS
                .iter()
                .map(|spec| staging.join(spec.hook.scripts_dir))
                .collect(),
            listings: vec![staging.join(".codex")],
            documents: PROVIDERS
                .iter()
                .map(|spec| spec.hook.external_file)
                .chain(codex.config_file())
                .map(|relative| staging.join(relative))
                .collect(),
            probes: Vec::new(),
        }
    }

    /// The files of a finished installer run in `staging` that the bundle `output` holds.
    pub(crate) fn capture_ops(&self, staging: Tree<'_>, output: &Path) -> Result<Vec<FileOp>> {
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
    staging: Tree<'_>,
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
    operations.extend(codex_script_ops(staging, output));
    Ok(operations)
}

/// Rewrites staged hook commands so they point at the bundle output.
struct CommandNormalization<'a> {
    staging: Tree<'a>,
    output: &'a Path,
    replacements: &'a [CommandReplacement],
}

impl CommandNormalization<'_> {
    fn capture_file(&self, relative: &str) -> Result<Option<FileOp>> {
        let source = self.staging.root().join(relative);
        let target = self.output.join(relative);
        match (self.staging.files().is_file(&source), is_json(&source)) {
            (false, _) => Ok(None),
            (true, true) => self.normalized_json_op(&source, &target).map(Some),
            (true, false) => Ok(Some(FileOp::CopyFile { source, target })),
        }
    }

    fn normalized_json_op(&self, source: &Path, target: &Path) -> Result<FileOp> {
        let mut value: Value = serde_json::from_str(
            self.staging
                .files()
                .read_text(source)
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
            self.staging.root().to_string_lossy().as_ref(),
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

fn capture_dir(relative: &str, staging: Tree<'_>, output: &Path) -> Option<FileOp> {
    let source = staging.root().join(relative);
    staging.files().is_dir(&source).then(|| FileOp::CopyTree {
        source,
        target: output.join(relative),
    })
}

fn codex_script_ops(staging: Tree<'_>, output: &Path) -> Vec<FileOp> {
    let source = staging.root().join(".codex");
    staging
        .files()
        .children(&source)
        .into_iter()
        .filter_map(|child| shell_script_op(child, output))
        .collect()
}

fn shell_script_op(child: Child, output: &Path) -> Option<FileOp> {
    let is_shell = child
        .path
        .extension()
        .and_then(|extension| extension.to_str())
        == Some("sh");
    (child.is_regular_file() && is_shell).then(|| FileOp::CopyFile {
        target: output.join(".codex").join(&child.name),
        source: child.path,
    })
}

fn manifest_op(output: &Path) -> Result<FileOp> {
    FileOp::write_json(
        output.join("hook_bundle.json"),
        &serde_json::json!({"version": BUNDLE_VERSION}),
    )
}
