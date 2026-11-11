use std::path::Path;

use anyhow::{Context, Result, bail, ensure};

use crate::{
    filesystem::{FileSystem, RealFileSystem, TemporaryDirectory},
    generation::{GenerationContext, claude_config, command_permissions, skills},
    layout::{InstalledLayout, SourceLayout},
    provider::{
        renderer::ProviderRenderer,
        spec::{ConfigSync, PROVIDERS, ProviderSpec},
    },
};

#[derive(Debug, Clone, Copy)]
pub(crate) enum Provider {
    Claude,
    Codex,
}

#[cfg(test)]
pub(crate) fn generate_claude_settings(source: &Path, out: &Path) -> Result<()> {
    crate::generation::claude_config::write_settings(source, out, &[])
}

#[cfg(test)]
pub(crate) fn generate_codex_config_source(source: &Path, out: &Path) -> Result<()> {
    crate::generation::codex_config::write_config_source(source, out, &[])
}

pub(crate) fn generate_skills(
    source: &Path,
    external_skills: &[skills::ExternalSkill],
    target: skills::SkillTarget<'_>,
) -> Result<()> {
    skills::render_skills(source, external_skills, target)
}

pub(crate) fn install(context: GenerationContext<'_>, out: &Path) -> Result<()> {
    let staging = TemporaryDirectory::create()?;
    carry_over_mutable_toml_configs(out, staging.path())?;
    render_installation(context, staging.path())?;
    preserve_claude_settings(out, staging.path())?;
    let plan = crate::install_plan::plan_install(
        &InstalledLayout::new(staging.path()),
        &InstalledLayout::new(out),
    );
    crate::install_executor::execute(&RealFileSystem, &plan)?;
    Ok(())
}

/// Seeds the staging tree with the installed TOML config, which rendering merges into.
fn carry_over_mutable_toml_configs(installed: &Path, staging: &Path) -> Result<()> {
    PROVIDERS
        .iter()
        .filter_map(|spec| spec.config)
        .filter(|config| config.sync == ConfigSync::MergeToml)
        .map(|config| config.path)
        .filter(|path| installed.join(path).exists())
        .try_for_each(|path| RealFileSystem.copy_file(&installed.join(path), &staging.join(path)))
}

/// Merges the generated Claude settings into the installed file.
///
/// `~/` stays unexpanded: the prefix is not always the home, and the Home Manager sync expands it.
fn preserve_claude_settings(installed: &Path, staging: &Path) -> Result<()> {
    let Some(path) = PROVIDERS
        .iter()
        .filter_map(|spec| spec.config)
        .find(|config| config.sync == ConfigSync::MergeClaudeSettings)
        .map(|config| config.path)
    else {
        return Ok(());
    };
    let generated = staging.join(format!("{path}.generated"));
    let target = staging.join(path);
    std::fs::rename(&target, &generated)
        .with_context(|| format!("failed to set aside {}", target.display()))?;
    if installed.join(path).exists() {
        RealFileSystem.copy_file(&installed.join(path), &target)?;
    }
    claude_config::merge_settings(&generated, &target)?;
    std::fs::remove_file(&generated)
        .with_context(|| format!("failed to remove {}", generated.display()))
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
    for (source, target) in installed.synced_directories_for(source_layout, providers) {
        sync_directory(&source, &target)?;
    }
    crate::install_executor::execute(
        &RealFileSystem,
        &crate::install_plan::plan_assets_for(out, providers),
    )?;
    for bundle in context.external_hooks {
        bundle.copy_assets(out)?;
    }
    let renderer = ProviderRenderer {
        context,
        providers,
        out,
    };
    providers.iter().try_for_each(|spec| renderer.render(spec))
}

/// Optional source directories that a profile omits install as empty directories.
fn sync_directory(source: &Path, target: &Path) -> Result<()> {
    if source.is_dir() {
        return RealFileSystem.replace_directory(source, target);
    }
    std::fs::create_dir_all(target)
        .with_context(|| format!("failed to create directory {}", target.display()))
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
