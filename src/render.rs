#![forbid(clippy::disallowed_methods, clippy::disallowed_types)]

use std::path::{Path, PathBuf};

use anyhow::{Result, bail, ensure};

use crate::{
    file_op::FileOp,
    generation::{
        GenerationContext, claude_config, command_permissions, external_hooks::ExternalHookBundle,
        skills,
    },
    layout::{InstalledLayout, SourceLayout},
    provider::{
        renderer::ProviderRenderer,
        spec::{ConfigSync, PROVIDERS, ProviderSpec},
    },
    runtime_root::RuntimeRoot,
    shell::{
        ExternalInputs, TemporaryDirectory, execute, load_snapshot, source_request, sync_documents,
    },
    snapshot::{ConfigFiles, Snapshot, SnapshotRequest, Tree},
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
    /// The computation context over the snapshot `files` taken for this request.
    fn context<'f>(&'f self, files: &'f Snapshot) -> GenerationContext<'f> {
        GenerationContext {
            dsh_profile: self.dsh_profile,
            source: Tree::new(self.source, files),
            external_hooks: self.external_hooks,
            runtime_root: self.runtime_root,
        }
    }

    fn external_inputs(&self) -> ExternalInputs<'_> {
        ExternalInputs {
            hook_bundles: self.external_hooks,
            skills: &[],
        }
    }
}

#[cfg(test)]
pub(crate) fn generate_claude_settings(source: &Path, out: &Path) -> Result<()> {
    let files = load_snapshot(&source_request(source, ExternalInputs::default()))?;
    execute(&[crate::generation::claude_config::settings_op(
        Tree::new(source, &files),
        out,
        &[],
    )?])
}

#[cfg(test)]
pub(crate) fn generate_codex_config_source(source: &Path, out: &Path) -> Result<()> {
    let files = load_snapshot(&source_request(source, ExternalInputs::default()))?;
    execute(&[crate::generation::codex_config::config_source_op(
        Tree::new(source, &files),
        out,
        &[],
    )?])
}

pub(crate) fn generate_skills(
    source: &Path,
    external_skills: &[skills::ExternalSkill],
    target: skills::SkillTarget<'_>,
) -> Result<()> {
    let external = ExternalInputs {
        skills: external_skills,
        ..ExternalInputs::default()
    };
    let files = load_snapshot(&source_request(source, external))?;
    execute(&[skills::render_skills(
        Tree::new(source, &files),
        external_skills,
        target,
    )?])
}

pub(crate) fn install(request: &RenderRequest<'_>, out: &Path) -> Result<()> {
    let staging = TemporaryDirectory::create()?;
    carry_over_mutable_configs(out, staging.path())?;
    render_installation(request, staging.path())?;
    preserve_claude_settings(out, staging.path())?;
    let plan = crate::install_plan::plan_install(
        &InstalledLayout::new(staging.path()),
        &InstalledLayout::new(out),
    );
    execute(&plan)
}

/// Seeds mutable configs that rendering merges into with their installed state.
fn carry_over_mutable_configs(installed: &Path, staging: &Path) -> Result<()> {
    let paths: Vec<&str> = PROVIDERS
        .iter()
        .filter(|spec| merges_into_installed_config(spec))
        .filter_map(ProviderSpec::config_path)
        .collect();
    let files = load_snapshot(&SnapshotRequest {
        probes: paths.iter().map(|path| installed.join(path)).collect(),
        ..SnapshotRequest::default()
    })?;
    let seeds: Vec<FileOp> = paths
        .into_iter()
        .filter(|path| files.exists(&installed.join(path)))
        .map(|path| FileOp::CopyFile {
            source: installed.join(path),
            target: staging.join(path),
        })
        .collect();
    execute(&seeds)
}

fn merges_into_installed_config(spec: &ProviderSpec) -> bool {
    let sync = spec.config.map(|config| config.sync);
    matches!(
        sync,
        Some(ConfigSync::ReplaceListedKeys | ConfigSync::MergeToml)
    )
}

/// Merges the generated Claude settings into the installed file.
///
/// `~/` stays unexpanded: the prefix is not always the home, and the Home Manager sync expands it.
fn preserve_claude_settings(installed: &Path, staging: &Path) -> Result<()> {
    let Some(path) = PROVIDERS
        .iter()
        .find(|spec| {
            spec.config
                .is_some_and(|config| config.sync == ConfigSync::ReplaceGeneratedKeys)
        })
        .and_then(ProviderSpec::config_path)
    else {
        return Ok(());
    };
    let generated = staging.join(path);
    let target = installed.join(path);
    let files = load_snapshot(&SnapshotRequest {
        documents: vec![generated.clone(), target.clone()],
        ..SnapshotRequest::default()
    })?;
    let config = ConfigFiles {
        generated: &generated,
        target: &target,
        files: &files,
    };
    execute(&[claude_config::merge_settings_op(config, &generated)?])
}

fn render_installation(request: &RenderRequest<'_>, out: &Path) -> Result<()> {
    render_installation_for(request, out, PROVIDERS)
}

pub(crate) fn render_installation_for(
    request: &RenderRequest<'_>,
    out: &Path,
    providers: &[ProviderSpec],
) -> Result<()> {
    let files = load_snapshot(&installation_request(request, out, providers))?;
    let context = request.context(&files);
    let source_layout = SourceLayout::new(context.source.root());
    command_permissions::validate_regex_policies(context.source)?;
    if let Some(missing) = source_layout
        .missing_required_directories(providers, &files)
        .first()
    {
        bail!(
            "missing required harness source directory: {}",
            missing.display()
        );
    }
    let directories = InstalledLayout::new(out).synced_directories_for(source_layout, providers);
    execute(&synced_directory_ops(directories, &files))?;
    execute(&crate::install_plan::plan_assets_for(out, providers))?;
    install_bundle_assets(context, out)?;
    let renderer = ProviderRenderer {
        context,
        providers,
        out,
    };
    for spec in providers {
        execute(&renderer.render(spec)?)?;
    }
    Ok(())
}

/// The source, the bundles, and the installed configs that rendering merges its output into.
fn installation_request(
    request: &RenderRequest<'_>,
    out: &Path,
    providers: &[ProviderSpec],
) -> SnapshotRequest {
    let mut snapshot = source_request(request.source, request.external_inputs());
    let merged_configs = providers
        .iter()
        .filter(|spec| merges_into_installed_config(spec))
        .filter_map(|spec| Some((spec.config?.sync, spec.config_file()?)));
    snapshot
        .documents
        .extend(merged_configs.flat_map(|(sync, path)| sync_documents(sync, &out.join(path))));
    snapshot
}

/// Copies each bundle's assets into `out`, which must not already hold them.
fn install_bundle_assets(context: GenerationContext<'_>, out: &Path) -> Result<()> {
    let files = context.source.files();
    let mut targets = Vec::new();
    for bundle in context.external_hooks {
        targets.extend(bundle.open(files)?.asset_install_paths()?);
    }
    let existing = load_snapshot(&SnapshotRequest {
        probes: targets.iter().map(|target| out.join(target)).collect(),
        ..SnapshotRequest::default()
    })?;
    for bundle in context.external_hooks {
        execute(&bundle.open(files)?.asset_ops(Tree::new(out, &existing))?)?;
    }
    Ok(())
}

/// Optional source directories that a profile omits install as empty directories.
fn synced_directory_ops(directories: Vec<(PathBuf, PathBuf)>, files: &Snapshot) -> Vec<FileOp> {
    directories
        .into_iter()
        .map(|(source, target)| synced_directory_op(source, target, files))
        .collect()
}

fn synced_directory_op(source: PathBuf, target: PathBuf, files: &Snapshot) -> FileOp {
    if files.is_dir(&source) {
        return FileOp::CopyTree { source, target };
    }
    FileOp::ReplaceTree {
        target,
        contents: Vec::new(),
    }
}

pub(crate) fn verify(root: &Path) -> Result<()> {
    let installed = InstalledLayout::new(root);
    let required_files = installed.verified_files_for(PROVIDERS);
    let required_directories = installed.verified_directories_for(PROVIDERS);
    let files = load_snapshot(&SnapshotRequest {
        probes: required_files
            .iter()
            .chain(&required_directories)
            .cloned()
            .collect(),
        ..SnapshotRequest::default()
    })?;
    for path in required_files {
        ensure!(
            files.is_file(&path),
            "missing or invalid harness file: {}",
            path.display()
        );
    }
    for path in required_directories {
        ensure!(
            files.is_dir(&path),
            "missing or invalid harness directory: {}",
            path.display()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests;
