use std::path::Path;

use anyhow::{Result, ensure};

use crate::{
    filesystem::{FileSystem, RealFileSystem, TemporaryDirectory},
    generation::{GenerationContext, command_permissions, skills},
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
        .filter(|spec| matches!(spec.config_sync, Some(ConfigSync::MergeToml)))
        .filter_map(|spec| spec.managed_config)
        .filter(|path| installed.join(path).exists())
        .try_for_each(|path| RealFileSystem.copy_file(&installed.join(path), &staging.join(path)))
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
    for (source, target) in
        installed.synced_directories_for(SourceLayout::new(context.source), providers)
    {
        RealFileSystem.replace_directory(&source, &target)?;
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
