use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::{Map, Value, map::Entry};

use crate::{
    provider::spec::ProviderSpec,
    source_model::{HookAsset, HookBundle, SourceDir},
};

const SUPPORTED_BUNDLE_VERSION: u64 = 1;

#[derive(Clone, Debug)]
pub(crate) struct ExternalHookBundle {
    name: HookBundleName,
    source: PathBuf,
}

impl ExternalHookBundle {
    /// The bundle directory, which the loader reads.
    pub(crate) fn source(&self) -> &Path {
        &self.source
    }

    pub(crate) fn name(&self) -> &str {
        self.name.as_str()
    }

    /// Checks the bundle manifest `text` read from `path`.
    ///
    /// # Errors
    ///
    /// Returns an error naming `path` when the manifest is invalid or its version unsupported.
    pub(crate) fn check_manifest(&self, path: &Path, text: &str) -> Result<()> {
        let manifest: HookBundleManifest = serde_json::from_str(text)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        ensure!(
            manifest.version == SUPPORTED_BUNDLE_VERSION,
            "unsupported external hook bundle version for {}: {}",
            self.name.as_str(),
            manifest.version,
        );
        Ok(())
    }

    /// The installed copy of each regular file under `scripts`, a bundle directory of one
    /// provider's hooks, in bundle order.
    ///
    /// # Errors
    ///
    /// Returns an error when a file is not under the directory.
    pub(crate) fn tree_assets(
        &self,
        scripts_dir: &Path,
        scripts: &SourceDir,
    ) -> Result<Vec<HookAsset>> {
        let target_root = self.asset_root(scripts_dir);
        scripts
            .files
            .iter()
            .map(|file| tree_asset(file.clone(), &scripts.path, &target_root))
            .collect()
    }

    /// The installed copy of each shell script that sits directly in the bundle's `.codex`.
    pub(crate) fn codex_script_assets(&self, scripts: &[PathBuf]) -> Vec<HookAsset> {
        let target_root = self.asset_root(Path::new(".codex/hooks"));
        scripts
            .iter()
            .filter_map(|script| script_asset(script, &target_root))
            .collect()
    }

    fn asset_root(&self, provider_hooks: &Path) -> PathBuf {
        provider_hooks.join("external").join(self.name.as_str())
    }
}

/// Where a bundle keeps the hook files of the providers, relative to its directory.
pub(crate) fn hook_file_paths(providers: &[ProviderSpec]) -> Vec<&'static str> {
    let declared = providers.iter().map(|spec| spec.hook.external_file);
    let mut seen = std::collections::BTreeSet::new();
    [CLAUDE_HOOK_FILE.path, CODEX_HOOK_FILE.path]
        .into_iter()
        .chain(declared)
        .filter(|path| seen.insert(*path))
        .collect()
}

/// Whether `path` names a shell script a bundle ships beside its Codex config.
pub(crate) fn is_codex_script(path: &Path) -> bool {
    is_shell_script(path)
}

pub(crate) fn merge_claude_hooks(bundle: &HookBundle, hooks: &mut Value) -> Result<()> {
    merge_hooks(bundle, hooks, &CLAUDE_HOOK_FILE)
}

pub(crate) fn merge_codex_hooks(bundle: &HookBundle, hooks: &mut Value) -> Result<()> {
    merge_hooks(bundle, hooks, &CODEX_HOOK_FILE)
}

pub(crate) fn merge_provider_hooks(
    bundle: &HookBundle,
    hooks: &mut Value,
    spec: &ProviderSpec,
) -> Result<()> {
    merge_hooks(
        bundle,
        hooks,
        &HookFile {
            path: spec.hook.external_file,
            target_key: Some("hooks"),
            events_key: None,
        },
    )
}

fn merge_hooks(bundle: &HookBundle, hooks: &mut Value, file: &HookFile) -> Result<()> {
    let Some(generated) = bundle.hook_files.get(file.path) else {
        return Ok(());
    };
    let relocation = CommandRelocation {
        assets: &bundle.assets,
        source: &bundle.root,
    };
    relocation
        .merge_events(file.target(hooks), file.events(generated))
        .with_context(|| format!("failed to merge external hook bundle {}", bundle.name))
}

const CLAUDE_HOOK_FILE: HookFile = HookFile {
    path: ".claude/settings.json",
    target_key: None,
    events_key: Some("hooks"),
};
const CODEX_HOOK_FILE: HookFile = HookFile {
    path: ".codex/hooks.json",
    target_key: Some("hooks"),
    events_key: Some("hooks"),
};
/// A provider hook file inside a bundle, where its events live in that file, and
/// where they merge into the generated provider config.
struct HookFile {
    path: &'static str,
    target_key: Option<&'static str>,
    events_key: Option<&'static str>,
}

impl HookFile {
    // Index only after the bundle file is known to exist: indexing a missing key
    // inserts null, which would later overwrite user-owned hooks on sync.
    fn target<'a>(&self, hooks: &'a mut Value) -> &'a mut Value {
        match self.target_key {
            Some(key) => &mut hooks[key],
            None => hooks,
        }
    }

    fn events<'a>(&self, generated: &'a Value) -> &'a Value {
        match self.events_key {
            Some(key) => &generated[key],
            None => generated,
        }
    }
}

/// Rewrites bundle hook commands so they point at the installed asset copies.
struct CommandRelocation<'a> {
    assets: &'a [HookAsset],
    source: &'a Path,
}

impl CommandRelocation<'_> {
    fn merge_events(&self, target: &mut Value, generated: &Value) -> Result<()> {
        let target = target
            .as_object_mut()
            .context("hook root must be a JSON object")?;
        let generated = generated
            .as_object()
            .context("external hooks must be a JSON object")?;
        generated
            .iter()
            .try_for_each(|(event, groups)| self.merge_event(target.entry(event), groups))
    }

    fn merge_event(&self, target: Entry<'_>, groups: &Value) -> Result<()> {
        let event = target.key().clone();
        let groups = groups
            .as_array()
            .with_context(|| format!("external hook event {event} must be an array"))?;
        let target_groups = target
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
            .with_context(|| format!("hook event {event} must be an array"))?;
        self.merge_groups(target_groups, groups)
    }

    fn merge_groups(&self, target_groups: &mut Vec<Value>, groups: &[Value]) -> Result<()> {
        groups
            .iter()
            .try_for_each(|group| self.merge_group(target_groups, group))
    }

    fn merge_group(&self, target_groups: &mut Vec<Value>, group: &Value) -> Result<()> {
        let mut group = group.clone();
        self.replace_command_paths(&mut group)?;
        push_unique(target_groups, group);
        Ok(())
    }

    fn replace_command_paths(&self, value: &mut Value) -> Result<()> {
        match value {
            Value::Array(items) => items
                .iter_mut()
                .try_for_each(|item| self.replace_command_paths(item)),
            Value::Object(object) => self.replace_object_command_paths(object),
            _ => Ok(()),
        }
    }

    fn replace_object_command_paths(&self, object: &mut Map<String, Value>) -> Result<()> {
        object
            .get_mut("command")
            .and_then(string_mut)
            .map_or(Ok(()), |command| self.relocate_command(command))?;
        object
            .values_mut()
            .try_for_each(|item| self.replace_command_paths(item))
    }

    fn relocate_command(&self, command: &mut String) -> Result<()> {
        *command = self.normalize_command(command);
        let source_prefix = format!("{}/", self.source.display());
        ensure!(
            !command.contains(&source_prefix),
            "command contains uncaptured external hook bundle path: {command}"
        );
        Ok(())
    }

    fn normalize_command(&self, command: &str) -> String {
        let mut relocated = command.to_owned();
        self.assets
            .iter()
            .for_each(|asset| relocated = relocate_asset(asset, &relocated));
        relocated
    }
}

fn string_mut(value: &mut Value) -> Option<&mut String> {
    match value {
        Value::String(string) => Some(string),
        _ => None,
    }
}

fn push_unique(values: &mut Vec<Value>, value: Value) {
    if !values.contains(&value) {
        values.push(value);
    }
}

impl FromStr for ExternalHookBundle {
    type Err = String;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        let (name, source) = value
            .split_once('=')
            .ok_or_else(|| "external hook bundle must use NAME=PATH format".to_string())?;
        let source = parse_bundle_source(source)?;
        Ok(Self {
            name: HookBundleName::parse(name).map_err(|error| error.to_string())?,
            source,
        })
    }
}

fn parse_bundle_source(source: &str) -> std::result::Result<PathBuf, String> {
    if source.is_empty() {
        return Err("external hook bundle path must not be empty".to_string());
    }
    Ok(PathBuf::from(source))
}

#[derive(Clone, Debug)]
struct HookBundleName(String);

impl HookBundleName {
    fn parse(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        let is_valid = !value.is_empty()
            && value
                .chars()
                .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-');
        ensure!(
            is_valid,
            "external hook bundle name must contain only lowercase ASCII letters, digits, and hyphens: {value}"
        );
        Ok(Self(value))
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct HookBundleManifest {
    version: u64,
}

fn script_asset(script: &Path, target_root: &Path) -> Option<HookAsset> {
    Some(HookAsset {
        source: script.to_path_buf(),
        target: target_root.join(script.file_name()?),
    })
}

fn relocate_asset(asset: &HookAsset, command: &str) -> String {
    let source = asset.source.to_string_lossy();
    let target = format!("$HOME/{}", asset.target.display());
    let quoted_target = format!("\"{target}\"");
    command
        .replace(&format!("'{source}'"), &quoted_target)
        .replace(&format!("\"{source}\""), &quoted_target)
        .replace(source.as_ref(), &target)
}

fn tree_asset(file: PathBuf, source: &Path, target_root: &Path) -> Result<HookAsset> {
    let suffix = file
        .strip_prefix(source)
        .with_context(|| format!("failed to strip prefix {}", source.display()))?;
    let target = target_root.join(suffix);
    Ok(HookAsset {
        source: file,
        target,
    })
}

fn is_shell_script(path: &Path) -> bool {
    path.extension().and_then(|extension| extension.to_str()) == Some("sh")
}
