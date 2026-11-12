//! Loading external hook bundles: each manifest checked, each hook file parsed, each asset listed.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::{Result, ensure};
use serde_json::Value;
use toml_edit::DocumentMut;

use super::{
    LoadInputs,
    files::{read_json, read_text, read_toml, shell_scripts},
    source_dir,
};
use crate::{
    generation::external_hooks::{self, ExternalHookBundle},
    provider::spec::ProviderSpec,
    source_model::{HookAsset, HookBundle},
};

pub(super) fn load_bundles(inputs: &LoadInputs<'_>) -> Result<Vec<HookBundle>> {
    inputs
        .hook_bundles
        .iter()
        .map(|bundle| load_bundle(bundle, inputs.providers))
        .collect()
}

pub(super) fn load_bundle(
    bundle: &ExternalHookBundle,
    providers: &[ProviderSpec],
) -> Result<HookBundle> {
    let root = bundle.source();
    let manifest = root.join("hook_bundle.json");
    bundle.check_manifest(&manifest, &read_text(&manifest)?)?;
    let assets = bundle_assets(bundle, providers)?;
    reject_duplicate_targets(bundle.name(), &assets)?;
    Ok(HookBundle {
        name: bundle.name().to_owned(),
        root: root.to_path_buf(),
        assets,
        hook_files: bundle_hook_files(root, providers)?,
        codex_config: bundle_codex_config(root)?,
    })
}

pub(super) fn bundle_hook_files(
    root: &Path,
    providers: &[ProviderSpec],
) -> Result<BTreeMap<String, Value>> {
    external_hooks::hook_file_paths(providers)
        .into_iter()
        .map(|relative| (relative, root.join(relative)))
        .filter(|(_, path)| path.is_file())
        .map(|(relative, path)| Ok((relative.to_owned(), read_json(&path)?)))
        .collect()
}

pub(super) fn bundle_codex_config(root: &Path) -> Result<Option<(PathBuf, DocumentMut)>> {
    let path = root.join(".codex/config.toml");
    if !path.is_file() {
        return Ok(None);
    }
    let document = read_toml(&path)?;
    Ok(Some((path, document)))
}

pub(super) fn bundle_assets(
    bundle: &ExternalHookBundle,
    providers: &[ProviderSpec],
) -> Result<Vec<HookAsset>> {
    let mut assets = Vec::new();
    for spec in providers {
        let scripts = source_dir(bundle.source().join(spec.hook.scripts_dir))?;
        assets.extend(bundle.tree_assets(Path::new(spec.hook.scripts_dir), &scripts)?);
    }
    let codex_scripts = shell_scripts(&bundle.source().join(".codex"))?;
    assets.extend(bundle.codex_script_assets(&codex_scripts));
    assets.sort_by(|left, right| left.target.cmp(&right.target));
    Ok(assets)
}

/// Two bundle files that land on one installed path would silently overwrite each other.
pub(super) fn reject_duplicate_targets(name: &str, assets: &[HookAsset]) -> Result<()> {
    let mut seen = BTreeSet::new();
    let duplicate = assets.iter().find(|asset| !seen.insert(&asset.target));
    ensure!(
        duplicate.is_none(),
        "external hook bundle {name} maps two files to {}",
        duplicate.map_or_else(String::new, |asset| asset.target.display().to_string()),
    );
    Ok(())
}
