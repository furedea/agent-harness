use std::path::Path;

use anyhow::{Context, Result, ensure};

use crate::{
    fs_ops,
    generation::{
        GenerationContext, claude_config, codex_config, command_permissions, hermes_config, hooks,
        protection, skills,
    },
    layout::{InstalledLayout, SourceLayout},
    provider::spec::{PROVIDERS, ProviderSpec},
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
    let staging = fs_ops::TemporaryDirectory::create()?;
    let rendered = InstalledLayout::new(staging.path());
    let installed = InstalledLayout::new(out);
    if installed.codex_config().exists() {
        fs_ops::copy_file(&installed.codex_config(), &rendered.codex_config())?;
    }
    render_installation(context, staging.path())?;
    for (source, target) in rendered
        .managed_directories()
        .into_iter()
        .zip(installed.managed_directories())
    {
        fs_ops::copy_dir(&source, &target)?;
    }
    for (source, target) in rendered
        .managed_files()
        .into_iter()
        .zip(installed.managed_files())
    {
        let bytes = std::fs::read(&source)
            .with_context(|| format!("failed to read rendered file {}", source.display()))?;
        fs_ops::write_file_atomically(&target, &bytes)?;
    }
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
    fs_ops::copy_file(
        &source_layout.agent_instructions(),
        &installed.codex_agent_instructions(),
    )?;
    fs_ops::copy_file(
        &source_layout.agent_instructions(),
        &installed.claude_agent_instructions(),
    )?;
    for (source, target) in installed.synced_directories(source_layout) {
        fs_ops::copy_dir(&source, &target)?;
    }

    fs_ops::copy_file(
        &source_layout.agent_instructions(),
        &installed.dsh_instructions(),
    )?;
    for spec in PROVIDERS {
        write_provider_assets(out, spec)?;
    }
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
    fs_ops::copy_dir(&installed.codex_skills(), &installed.dsh_skills())?;
    hermes_config::write_managed_config(&installed.hermes_managed_config())?;

    Ok(())
}

fn write_provider_assets(out: &Path, spec: &ProviderSpec) -> Result<()> {
    let Some(bridge) = spec.bridge_path else {
        return Ok(());
    };
    for asset in spec.assets {
        let path = provider_asset_path(out, bridge, asset.name);
        fs_ops::write_file_atomically(&path, asset.content.as_bytes())?;
    }
    Ok(())
}

fn provider_asset_path(out: &Path, bridge: &str, name: &str) -> std::path::PathBuf {
    if Path::new(bridge)
        .file_name()
        .is_some_and(|file| file == name)
    {
        out.join(bridge)
    } else {
        out.join(bridge).join(name)
    }
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
        .filter_map(|spec| spec.bridge_path.map(|bridge| (spec, bridge)))
        .flat_map(|(spec, bridge)| {
            spec.assets
                .iter()
                .map(move |asset| provider_asset_path(root, bridge, asset.name))
        })
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
