use std::path::{Path, PathBuf};

use anyhow::{Result, bail, ensure};

use crate::{
    file_op::FileOp,
    generation::{GenerationContext, claude_config, command_permissions, skills},
    layout::{InstalledLayout, SourceLayout},
    provider::{
        renderer::ProviderRenderer,
        spec::{ConfigSync, PROVIDERS, ProviderSpec},
    },
    shell::{TemporaryDirectory, execute},
};

#[derive(Debug, Clone, Copy)]
pub(crate) enum Provider {
    Claude,
    Codex,
}

#[cfg(test)]
pub(crate) fn generate_claude_settings(source: &Path, out: &Path) -> Result<()> {
    execute(&[crate::generation::claude_config::settings_op(
        source,
        out,
        &[],
    )?])
}

#[cfg(test)]
pub(crate) fn generate_codex_config_source(source: &Path, out: &Path) -> Result<()> {
    execute(&[crate::generation::codex_config::config_source_op(
        source,
        out,
        &[],
    )?])
}

pub(crate) fn generate_skills(
    source: &Path,
    external_skills: &[skills::ExternalSkill],
    target: skills::SkillTarget<'_>,
) -> Result<()> {
    execute(&[skills::render_skills(source, external_skills, target)?])
}

pub(crate) fn install(context: GenerationContext<'_>, out: &Path) -> Result<()> {
    let staging = TemporaryDirectory::create()?;
    carry_over_mutable_configs(out, staging.path())?;
    render_installation(context, staging.path())?;
    preserve_claude_settings(out, staging.path())?;
    let plan = crate::install_plan::plan_install(
        &InstalledLayout::new(staging.path()),
        &InstalledLayout::new(out),
    );
    execute(&plan)
}

/// Seeds mutable configs that rendering merges into with their installed state.
fn carry_over_mutable_configs(installed: &Path, staging: &Path) -> Result<()> {
    let seeds: Vec<FileOp> = PROVIDERS
        .iter()
        .filter(|spec| merges_into_installed_config(spec))
        .filter_map(ProviderSpec::config_path)
        .filter(|path| installed.join(path).exists())
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
    let merged = claude_config::merge_settings_op(&generated, &installed.join(path), &generated)?;
    execute(&[merged])
}

fn render_installation(context: GenerationContext<'_>, out: &Path) -> Result<()> {
    render_installation_for(context, out, PROVIDERS)
}

pub(crate) fn render_installation_for(
    context: GenerationContext<'_>,
    out: &Path,
    providers: &[ProviderSpec],
) -> Result<()> {
    let installed = InstalledLayout::new(out);
    command_permissions::validate_regex_policies(context.source)?;
    let source_layout = SourceLayout::new(context.source);
    if let Some(missing) = source_layout
        .missing_required_directories(providers)
        .first()
    {
        bail!(
            "missing required harness source directory: {}",
            missing.display()
        );
    }
    execute(&synced_directory_ops(&installed, source_layout, providers))?;
    execute(&crate::install_plan::plan_assets_for(out, providers))?;
    for bundle in context.external_hooks {
        execute(&bundle.asset_ops(out)?)?;
    }
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

/// Optional source directories that a profile omits install as empty directories.
fn synced_directory_ops(
    installed: &InstalledLayout<'_>,
    source_layout: SourceLayout<'_>,
    providers: &[ProviderSpec],
) -> Vec<FileOp> {
    installed
        .synced_directories_for(source_layout, providers)
        .into_iter()
        .map(|(source, target)| synced_directory_op(source, target))
        .collect()
}

fn synced_directory_op(source: PathBuf, target: PathBuf) -> FileOp {
    if source.is_dir() {
        return FileOp::CopyTree { source, target };
    }
    FileOp::ReplaceTree {
        target,
        contents: Vec::new(),
    }
}

pub(crate) fn verify(root: &Path) -> Result<()> {
    let installed = InstalledLayout::new(root);
    for path in installed.verified_files_for(PROVIDERS) {
        ensure!(
            path.is_file(),
            "missing or invalid harness file: {}",
            path.display()
        );
    }
    for path in installed.verified_directories_for(PROVIDERS) {
        ensure!(
            path.is_dir(),
            "missing or invalid harness directory: {}",
            path.display()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests;
