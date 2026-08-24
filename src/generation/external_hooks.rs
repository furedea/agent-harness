use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde_json::Value;

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
        for asset in self.assets()? {
            self.copy_asset(&asset.source, &output.join(asset.target))?;
        }
        Ok(())
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
        self.validate()?;
        let path = self.source.join(".claude/settings.json");
        if !path.is_file() {
            return Ok(());
        }

        let generated = read_json(&path)?;
        merge_events(hooks, &generated["hooks"], &self.assets()?, &self.source).with_context(|| {
            format!(
                "failed to merge external hook bundle {}",
                self.name.as_str()
            )
        })
    }

    pub(crate) fn merge_codex_hooks(&self, hooks: &mut Value) -> Result<()> {
        self.validate()?;
        let path = self.source.join(".codex/hooks.json");
        if !path.is_file() {
            return Ok(());
        }

        let generated = read_json(&path)?;
        merge_events(
            &mut hooks["hooks"],
            &generated["hooks"],
            &self.assets()?,
            &self.source,
        )
        .with_context(|| {
            format!(
                "failed to merge external hook bundle {}",
                self.name.as_str()
            )
        })
    }

    fn validate(&self) -> Result<()> {
        let path = self.source.join("hook_bundle.json");
        let manifest: HookBundleManifest = serde_json::from_str(
            &std::fs::read_to_string(&path)
                .with_context(|| format!("failed to read {}", path.display()))?,
        )
        .with_context(|| format!("failed to parse {}", path.display()))?;
        if manifest.version != SUPPORTED_BUNDLE_VERSION {
            bail!(
                "unsupported external hook bundle version for {}: {}",
                self.name.as_str(),
                manifest.version,
            );
        }
        Ok(())
    }

    fn assets(&self) -> Result<Vec<HookAsset>> {
        let mut assets = Vec::new();
        self.collect_asset_tree(Path::new(".claude/hooks"), &mut assets)?;
        self.collect_asset_tree(Path::new(".codex/hooks"), &mut assets)?;
        self.collect_codex_scripts(&mut assets)?;
        assets.sort_by(|left, right| left.target.cmp(&right.target));
        Ok(assets)
    }

    fn collect_asset_tree(&self, relative: &Path, assets: &mut Vec<HookAsset>) -> Result<()> {
        let source = self.source.join(relative);
        if !source.is_dir() {
            return Ok(());
        }
        for file in fs_ops::regular_files(&source)? {
            let suffix = file
                .strip_prefix(&source)
                .with_context(|| format!("failed to strip prefix {}", source.display()))?
                .to_path_buf();
            assets.push(HookAsset {
                source: file,
                target: self.asset_root(relative).join(suffix),
            });
        }
        Ok(())
    }

    fn collect_codex_scripts(&self, assets: &mut Vec<HookAsset>) -> Result<()> {
        let source = self.source.join(".codex");
        if !source.is_dir() {
            return Ok(());
        }
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
                assets.push(HookAsset {
                    source: path,
                    target: self
                        .asset_root(Path::new(".codex/hooks"))
                        .join(entry.file_name()),
                });
            }
        }
        Ok(())
    }

    fn asset_root(&self, provider_hooks: &Path) -> PathBuf {
        provider_hooks.join("external").join(self.name.as_str())
    }

    fn copy_asset(&self, source: &Path, target: &Path) -> Result<()> {
        if target.exists() {
            bail!(
                "external hook bundle {} conflicts with {}",
                self.name.as_str(),
                target.display(),
            );
        }
        fs_ops::copy_file(source, target)
    }
}

#[derive(Debug)]
struct HookAsset {
    source: PathBuf,
    target: PathBuf,
}

impl FromStr for ExternalHookBundle {
    type Err = String;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        let (name, source) = value
            .split_once('=')
            .ok_or_else(|| "external hook bundle must use NAME=PATH format".to_string())?;
        if source.is_empty() {
            return Err("external hook bundle path must not be empty".to_string());
        }
        Ok(Self {
            name: HookBundleName::parse(name).map_err(|error| error.to_string())?,
            source: PathBuf::from(source),
        })
    }
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
        if !is_valid {
            bail!(
                "external hook bundle name must contain only lowercase ASCII letters, digits, and hyphens: {value}"
            );
        }
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

fn merge_events(
    target: &mut Value,
    generated: &Value,
    assets: &[HookAsset],
    source: &Path,
) -> Result<()> {
    let target = target
        .as_object_mut()
        .context("hook root must be a JSON object")?;
    let generated = generated
        .as_object()
        .context("external hooks must be a JSON object")?;

    for (event, groups) in generated {
        let groups = groups
            .as_array()
            .with_context(|| format!("external hook event {event} must be an array"))?;
        let target_groups = target
            .entry(event.clone())
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
            .with_context(|| format!("hook event {event} must be an array"))?;
        for group in groups {
            let mut group = group.clone();
            replace_command_paths(&mut group, assets, source)?;
            if !target_groups.contains(&group) {
                target_groups.push(group);
            }
        }
    }
    Ok(())
}

fn replace_command_paths(value: &mut Value, assets: &[HookAsset], source: &Path) -> Result<()> {
    match value {
        Value::Array(items) => {
            for item in items {
                replace_command_paths(item, assets, source)?;
            }
        }
        Value::Object(object) => {
            if let Some(Value::String(command)) = object.get_mut("command") {
                *command = normalize_command(command, assets);
                let source_prefix = format!("{}/", source.display());
                if command.contains(&source_prefix) {
                    bail!("command contains uncaptured external hook bundle path: {command}");
                }
            }
            for item in object.values_mut() {
                replace_command_paths(item, assets, source)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn normalize_command(command: &str, assets: &[HookAsset]) -> String {
    assets
        .iter()
        .fold(command.to_string(), relocate_asset_command)
}

fn relocate_asset_command(command: String, asset: &HookAsset) -> String {
    let source = asset.source.to_string_lossy();
    let target = format!("$HOME/{}", asset.target.display());
    let quoted_target = format!("\"{target}\"");
    command
        .replace(&format!("'{source}'"), &quoted_target)
        .replace(&format!("\"{source}\""), &quoted_target)
        .replace(source.as_ref(), &target)
}

fn read_json(path: &Path) -> Result<Value> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&content).with_context(|| format!("failed to parse {}", path.display()))
}
