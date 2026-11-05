use std::path::Path;

use anyhow::{Result, ensure};

use crate::{
    filesystem::{FileSystem, RealFileSystem, TemporaryDirectory},
    generation::{
        GenerationContext, claude_config, codex_config, command_permissions, hermes_config, hooks,
        protection, skills,
    },
    layout::{InstalledLayout, SourceLayout},
    provider::spec::PROVIDERS,
};

#[derive(Debug, Clone, Copy)]
pub(crate) enum Provider {
    Claude,
    Codex,
}

#[cfg(test)]
pub(crate) fn generate_claude_settings(source: &Path, out: &Path) -> Result<()> {
    claude_config::write_settings(source, out, &[])
}

#[cfg(test)]
pub(crate) fn generate_codex_config_source(source: &Path, out: &Path) -> Result<()> {
    codex_config::write_config_source(source, out, &[])
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
    let rendered = InstalledLayout::new(staging.path());
    let installed = InstalledLayout::new(out);
    if installed.codex_config().exists() {
        crate::filesystem::FileSystem::copy_file(
            &crate::filesystem::RealFileSystem,
            &installed.codex_config(),
            &rendered.codex_config(),
        )?;
    }
    render_installation(context, staging.path())?;
    let plan = crate::install_plan::plan_install(&rendered, &installed);
    crate::install_executor::execute(&crate::filesystem::RealFileSystem, &plan)?;
    Ok(())
}

fn render_installation(context: GenerationContext<'_>, out: &Path) -> Result<()> {
    let GenerationContext {
        source,
        external_hooks,
        ..
    } = context;
    let installed = InstalledLayout::new(out);
    let source_layout = SourceLayout::new(source);
    command_permissions::validate_regex_policies(source)?;
    RealFileSystem.copy_file(
        &source_layout.agent_instructions(),
        &installed.codex_agent_instructions(),
    )?;
    RealFileSystem.copy_file(
        &source_layout.agent_instructions(),
        &installed.claude_agent_instructions(),
    )?;
    for (source, target) in installed.synced_directories(source_layout) {
        RealFileSystem.replace_directory(&source, &target)?;
    }

    RealFileSystem.copy_file(
        &source_layout.agent_instructions(),
        &installed.dsh_instructions(),
    )?;
    crate::install_executor::execute(&RealFileSystem, &crate::install_plan::plan_assets(out))?;
    for bundle in external_hooks {
        bundle.copy_assets(out)?;
    }
    hooks::write_codex_hooks_for_runtime(context, &installed.codex_hook_config())?;
    generate_skills(
        source,
        &[],
        skills::SkillTarget {
            provider: Provider::Codex,
            path: &installed.codex_skills(),
        },
    )?;
    generate_skills(
        source,
        &[],
        skills::SkillTarget {
            provider: Provider::Claude,
            path: &installed.claude_skills(),
        },
    )?;
    claude_config::write_settings_for_runtime(context, &installed.claude_settings())?;
    command_permissions::write_codex_rules(source, &installed.codex_rules())?;
    command_permissions::write_runtime_policy(source, &installed.claude_command_permissions())?;
    protection::write_runtime_policy(
        protection::protected_paths_for_runtime(context)?,
        &installed.claude_protected_paths(),
    )?;
    codex_config::sync_managed_document(
        &codex_config::generated_config_source(context)?,
        &installed.codex_config(),
    )?;
    for spec in PROVIDERS {
        hooks::write_provider_hooks_for_runtime(context, &out.join(spec.hook_manifest), spec)?;
    }
    RealFileSystem.replace_directory(&installed.codex_skills(), &installed.dsh_skills())?;
    hermes_config::write_managed_config(&installed.hermes_managed_config())?;

    Ok(())
}

pub(crate) fn verify(root: &Path) -> Result<()> {
    let installed = InstalledLayout::new(root);
    for path in [
        installed.codex_agent_instructions(),
        installed.codex_config(),
        installed.codex_hook_config(),
        installed.codex_rules(),
        installed.claude_agent_instructions(),
        installed.claude_allowed_command_rules(),
        installed.claude_command_permissions(),
        installed.claude_forbidden_command_rules(),
        installed.claude_protected_paths(),
        installed.claude_secret_commit_policy(),
        installed.claude_secret_path_policy(),
        installed.claude_settings(),
        installed.hermes_managed_config(),
        installed.dsh_instructions(),
    ]
    .into_iter()
    .chain(PROVIDERS.iter().map(|spec| root.join(spec.hook_manifest)))
    {
        ensure!(
            path.is_file(),
            "missing or invalid harness file: {}",
            path.display()
        );
    }
    for path in PROVIDERS
        .iter()
        .flat_map(|spec| spec.assets.iter().map(|asset| root.join(asset.destination)))
    {
        ensure!(
            path.is_file(),
            "missing or invalid harness file: {}",
            path.display()
        );
    }
    for path in [installed.codex_skills(), installed.claude_skills()] {
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
