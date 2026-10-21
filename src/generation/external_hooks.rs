use std::ffi::OsString;
use std::fs::DirEntry;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::{Map, Value, map::Entry};

use crate::fs_ops;

const SUPPORTED_BUNDLE_VERSION: u64 = 1;

#[derive(Clone, Debug)]
pub(crate) struct ExternalHookBundle {
    name: HookBundleName,
    source: PathBuf,
}

impl ExternalHookBundle {
    pub(crate) fn copy_assets(&self, output: &Path) -> Result<()> {
        self.validate()?;
        self.assets()?
            .iter()
            .try_for_each(|asset| self.copy_asset(&asset.source, &output.join(&asset.target)))
    }

    pub(crate) fn asset_install_paths(&self) -> Result<Vec<PathBuf>> {
        self.validate()?;
        Ok(self
            .assets()?
            .into_iter()
            .map(|asset| asset.target)
            .collect())
    }

    pub(crate) fn codex_config_path(&self) -> Result<Option<PathBuf>> {
        self.validate()?;
        let path = self.source.join(".codex/config.toml");
        Ok(path.is_file().then_some(path))
    }

    pub(crate) fn merge_claude_hooks(&self, hooks: &mut Value) -> Result<()> {
        self.merge_hooks(hooks, &CLAUDE_HOOK_FILE)
    }

    pub(crate) fn merge_codex_hooks(&self, hooks: &mut Value) -> Result<()> {
        self.merge_hooks(hooks, &CODEX_HOOK_FILE)
    }

    pub(crate) fn merge_devin_hooks(&self, hooks: &mut Value) -> Result<()> {
        self.merge_hooks(hooks, &DEVIN_HOOK_FILE)
    }

    pub(crate) fn merge_hermes_hooks(&self, hooks: &mut Value) -> Result<()> {
        self.merge_hooks(hooks, &HERMES_HOOK_FILE)
    }

    pub(crate) fn merge_pi_hooks(&self, hooks: &mut Value) -> Result<()> {
        self.merge_hooks(hooks, &PI_HOOK_FILE)
    }

    fn merge_hooks(&self, hooks: &mut Value, file: &HookFile) -> Result<()> {
        self.validate()?;
        match read_optional_json(&self.source.join(file.path))? {
            Some(generated) => self.merge_generated(file.target(hooks), file.events(&generated)),
            None => Ok(()),
        }
    }

    fn merge_generated(&self, target: &mut Value, events: &Value) -> Result<()> {
        let relocation = CommandRelocation {
            assets: &self.assets()?,
            source: &self.source,
        };
        relocation
            .merge_events(target, events)
            .with_context(|| self.merge_failure())
    }

    fn merge_failure(&self) -> String {
        format!(
            "failed to merge external hook bundle {}",
            self.name.as_str()
        )
    }

    fn validate(&self) -> Result<()> {
        let path = self.source.join("hook_bundle.json");
        let manifest: HookBundleManifest = serde_json::from_str(
            &std::fs::read_to_string(&path)
                .with_context(|| format!("failed to read {}", path.display()))?,
        )
        .with_context(|| format!("failed to parse {}", path.display()))?;
        ensure!(
            manifest.version == SUPPORTED_BUNDLE_VERSION,
            "unsupported external hook bundle version for {}: {}",
            self.name.as_str(),
            manifest.version,
        );
        Ok(())
    }

    fn assets(&self) -> Result<Vec<HookAsset>> {
        let mut assets = PROVIDER_HOOK_DIRS
            .iter()
            .map(|relative| self.asset_tree(Path::new(relative)))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        assets.extend(self.codex_scripts()?);
        assets.sort_by(|left, right| left.target.cmp(&right.target));
        Ok(assets)
    }

    fn asset_tree(&self, relative: &Path) -> Result<Vec<HookAsset>> {
        let source = self.source.join(relative);
        let target_root = self.asset_root(relative);
        existing_regular_files(&source)?
            .into_iter()
            .map(|file| tree_asset(file, &source, &target_root))
            .collect()
    }

    fn codex_scripts(&self) -> Result<Vec<HookAsset>> {
        let target_root = self.asset_root(Path::new(".codex/hooks"));
        Ok(shell_scripts(&self.source.join(".codex"))?
            .into_iter()
            .map(|(source, file_name)| HookAsset {
                source,
                target: target_root.join(file_name),
            })
            .collect())
    }

    fn asset_root(&self, provider_hooks: &Path) -> PathBuf {
        provider_hooks.join("external").join(self.name.as_str())
    }

    fn copy_asset(&self, source: &Path, target: &Path) -> Result<()> {
        ensure!(
            !target.exists(),
            "external hook bundle {} conflicts with {}",
            self.name.as_str(),
            target.display(),
        );
        fs_ops::copy_file(source, target)
    }
}

const PROVIDER_HOOK_DIRS: [&str; 5] = [
    ".claude/hooks",
    ".codex/hooks",
    ".devin/hooks",
    ".hermes/hooks",
    ".pi/hooks",
];

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
const DEVIN_HOOK_FILE: HookFile = HookFile {
    path: ".devin/hooks.v1.json",
    target_key: Some("hooks"),
    events_key: None,
};
const HERMES_HOOK_FILE: HookFile = HookFile {
    path: ".hermes/hooks.json",
    target_key: Some("hooks"),
    events_key: None,
};
const PI_HOOK_FILE: HookFile = HookFile {
    path: ".pi/hooks.json",
    target_key: Some("hooks"),
    events_key: None,
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
            .for_each(|asset| relocated = asset.relocate(&relocated));
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

#[derive(Debug)]
struct HookAsset {
    source: PathBuf,
    target: PathBuf,
}

impl HookAsset {
    fn relocate(&self, command: &str) -> String {
        let source = self.source.to_string_lossy();
        let target = format!("$HOME/{}", self.target.display());
        let quoted_target = format!("\"{target}\"");
        command
            .replace(&format!("'{source}'"), &quoted_target)
            .replace(&format!("\"{source}\""), &quoted_target)
            .replace(source.as_ref(), &target)
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

fn existing_regular_files(dir: &Path) -> Result<Vec<PathBuf>> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    fs_ops::regular_files(dir)
}

fn shell_scripts(dir: &Path) -> Result<Vec<(PathBuf, OsString)>> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut scripts = Vec::new();
    for entry in
        std::fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))?
    {
        let entry = entry.with_context(|| format!("failed to read entry in {}", dir.display()))?;
        scripts.extend(shell_script(&entry)?);
    }
    Ok(scripts)
}

fn shell_script(entry: &DirEntry) -> Result<Option<(PathBuf, OsString)>> {
    let path = entry.path();
    let is_file = entry
        .file_type()
        .with_context(|| format!("failed to inspect {}", path.display()))?
        .is_file();
    let is_shell = path.extension().and_then(|extension| extension.to_str()) == Some("sh");
    Ok((is_file && is_shell).then(|| (path, entry.file_name())))
}

fn read_optional_json(path: &Path) -> Result<Option<Value>> {
    if !path.is_file() {
        return Ok(None);
    }
    read_json(path).map(Some)
}

fn read_json(path: &Path) -> Result<Value> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&content).with_context(|| format!("failed to parse {}", path.display()))
}
