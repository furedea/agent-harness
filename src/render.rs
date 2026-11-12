#![forbid(clippy::disallowed_methods, clippy::disallowed_types)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};

use crate::{
    file_op::FileOp,
    generation::{GenerationContext, external_hooks::ExternalHookBundle, skills},
    install_plan::InstallPlan,
    layout::{InstalledLayout, SourceLayout},
    provider::{
        renderer::ProviderRenderer,
        spec::{PROVIDERS, ProviderSpec},
    },
    runtime_root::RuntimeRoot,
    shell::{
        LoadInputs, TemporaryDirectory, execute, execute_stages, load_installed, load_source,
        verify_paths,
    },
    source_model::{HookAsset, HookBundle, Source},
};

#[derive(Debug, Clone, Copy)]
pub(crate) enum Provider {
    Claude,
    Codex,
}

/// What a rendering run is asked to do, before any of it is read from the disk.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RenderRequest<'a> {
    pub(crate) dsh_profile: &'a str,
    pub(crate) source: &'a Path,
    pub(crate) external_hooks: &'a [ExternalHookBundle],
    pub(crate) runtime_root: &'a RuntimeRoot,
}

impl RenderRequest<'_> {
    fn load_inputs<'p>(&'p self, providers: &'p [ProviderSpec]) -> LoadInputs<'p> {
        LoadInputs {
            providers,
            hook_bundles: self.external_hooks,
            skills: &[],
        }
    }

    /// The computation context over the source loaded for this request.
    fn context<'s>(&'s self, source: &'s Source) -> GenerationContext<'s> {
        GenerationContext {
            dsh_profile: self.dsh_profile,
            source,
            runtime_root: self.runtime_root,
        }
    }
}

#[cfg(test)]
pub(crate) fn generate_claude_settings(source: &Path, out: &Path) -> Result<()> {
    let source = load_source(source, &LoadInputs::new(PROVIDERS))?;
    execute(&[crate::generation::claude_config::settings_op(&source, out)?])
}

#[cfg(test)]
pub(crate) fn generate_codex_config_source(source: &Path, out: &Path) -> Result<()> {
    let source = load_source(source, &LoadInputs::new(PROVIDERS))?;
    execute(&[crate::generation::codex_config::config_source_op(
        &source, out,
    )?])
}

pub(crate) fn generate_skills(
    source: &Path,
    external_skills: &[skills::ExternalSkill],
    target: skills::SkillTarget<'_>,
) -> Result<()> {
    let inputs = LoadInputs {
        skills: external_skills,
        ..LoadInputs::new(PROVIDERS)
    };
    let source = load_source(source, &inputs)?;
    execute(&[skills::render_skills(&source, target)?])
}

pub(crate) fn install(request: &RenderRequest<'_>, out: &Path) -> Result<()> {
    let source = load_source(request.source, &request.load_inputs(PROVIDERS))?;
    let installed = load_installed(out, PROVIDERS)?;
    let staging = TemporaryDirectory::create()?;
    let renderer = ProviderRenderer {
        context: request.context(&source),
        providers: PROVIDERS,
        installed: &installed,
        out: staging.path(),
    };
    let mut plan = plan_installation(&renderer)?;
    plan.push_stage(crate::install_plan::plan_install(
        &InstalledLayout::new(staging.path()),
        &InstalledLayout::new(out),
    ));
    execute_stages(&plan)
}

#[cfg(test)]
pub(crate) fn render_installation_for(
    request: &RenderRequest<'_>,
    out: &Path,
    providers: &[ProviderSpec],
) -> Result<()> {
    let source = load_source(request.source, &request.load_inputs(providers))?;
    let installed = load_installed(out, providers)?;
    let renderer = ProviderRenderer {
        context: request.context(&source),
        providers,
        installed: &installed,
        out,
    };
    execute_stages(&plan_installation(&renderer)?)
}

/// Every operation that renders the installation of `providers` into `out`.
///
/// The plan follows from the source and the installed configs alone: a provider that reuses
/// another's output computes it again, and the files a bundle's assets must not collide with are
/// worked out from the stages before.
fn plan_installation(renderer: &ProviderRenderer<'_>) -> Result<InstallPlan> {
    let source = renderer.context.source;
    let layout = InstalledLayout::new(renderer.out);
    let mut plan = InstallPlan::default();
    plan.push_stage(synced_directory_ops(
        source,
        layout.synced_directories_for(SourceLayout::new(source.root()), renderer.providers),
    )?);
    plan.push_stage(crate::install_plan::plan_assets_for(
        renderer.out,
        renderer.providers,
    ));
    plan.push_stage(bundle_asset_ops(&plan, source, renderer.out)?);
    for spec in renderer.providers {
        plan.push_stage(renderer.render(spec)?);
    }
    Ok(plan)
}

/// Optional source directories that a profile omits install as empty directories.
fn synced_directory_ops(
    source: &Source,
    directories: Vec<(PathBuf, PathBuf)>,
) -> Result<Vec<FileOp>> {
    directories
        .into_iter()
        .map(|(from, target)| synced_directory_op(source, &from, target))
        .collect()
}

fn synced_directory_op(source: &Source, from: &Path, target: PathBuf) -> Result<FileOp> {
    let directory = source
        .directory(from)
        .with_context(|| format!("source directory {} was not loaded", from.display()))?;
    Ok(directory.publish_to(target))
}

/// Copies each bundle's assets into `out`, which must not already hold them.
///
/// Each bundle looks at what the stages before and the bundles before it leave in `out`, so two
/// bundles that map to one path conflict.
fn bundle_asset_ops(plan: &InstallPlan, source: &Source, out: &Path) -> Result<Vec<FileOp>> {
    let mut occupied = plan.planned_files(source);
    let mut operations = Vec::new();
    for bundle in &source.hook_bundles {
        operations.extend(checked_asset_ops(bundle, out, &occupied)?);
        occupied.extend(bundle.assets.iter().map(|asset| out.join(&asset.target)));
    }
    Ok(operations)
}

fn checked_asset_ops(
    bundle: &HookBundle,
    out: &Path,
    occupied: &BTreeSet<PathBuf>,
) -> Result<Vec<FileOp>> {
    bundle
        .assets
        .iter()
        .map(|asset| {
            let target = out.join(&asset.target);
            ensure!(
                !is_occupied(occupied, &target),
                "external hook bundle {} conflicts with {}",
                bundle.name,
                target.display(),
            );
            Ok(asset_op(asset, &target))
        })
        .collect()
}

fn is_occupied(occupied: &BTreeSet<PathBuf>, target: &Path) -> bool {
    occupied.iter().any(|path| path.starts_with(target))
}

fn asset_op(asset: &HookAsset, target: &Path) -> FileOp {
    FileOp::CopyFile {
        source: asset.source.clone(),
        target: target.to_path_buf(),
    }
}

pub(crate) fn verify(root: &Path) -> Result<()> {
    let installed = InstalledLayout::new(root);
    verify_paths(
        &installed.verified_files_for(PROVIDERS),
        &installed.verified_directories_for(PROVIDERS),
    )
}

#[cfg(test)]
mod tests;
