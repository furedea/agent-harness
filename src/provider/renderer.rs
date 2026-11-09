use std::path::Path;

use anyhow::{Result, bail};

use crate::{
    filesystem::{FileSystem, RealFileSystem},
    generation::{
        GenerationContext, claude_config, codex_config, command_permissions, hermes_config, hooks,
        protection, skills,
    },
    layout::SourceLayout,
    provider::spec::{ProviderId, ProviderSpec, Skills, SkillsSource},
    render::Provider,
};

/// Provider functions: what each provider's declared files contain.
///
/// The registry declares which files a provider owns; this renders them. Every `match` on
/// `ProviderId` is exhaustive, so adding a provider fails to compile until it is rendered.
pub(crate) struct ProviderRenderer<'a> {
    pub(crate) context: GenerationContext<'a>,
    pub(crate) providers: &'a [ProviderSpec],
    pub(crate) out: &'a Path,
}

impl ProviderRenderer<'_> {
    pub(crate) fn render(&self, spec: &ProviderSpec) -> Result<()> {
        self.copy_instructions(spec)?;
        self.write_hook_manifest(spec)?;
        spec.skills
            .map(|skills| self.render_skills(skills))
            .transpose()?;
        self.write_generated_files(spec)?;
        spec.managed_config
            .map(|path| self.write_managed_config(spec.id, &self.out.join(path)))
            .transpose()?;
        Ok(())
    }

    fn copy_instructions(&self, spec: &ProviderSpec) -> Result<()> {
        let source = SourceLayout::new(self.context.source).agent_instructions();
        spec.instructions
            .map(|path| RealFileSystem.copy_file(&source, &self.out.join(path)))
            .transpose()?;
        Ok(())
    }

    fn write_hook_manifest(&self, spec: &ProviderSpec) -> Result<()> {
        let path = self.out.join(spec.hook_manifest);
        match spec.id {
            ProviderId::Claude => claude_config::write_settings_for_runtime(self.context, &path),
            ProviderId::Codex
            | ProviderId::Devin
            | ProviderId::Hermes
            | ProviderId::Pi
            | ProviderId::Dsh => hooks::write_hooks_for_runtime(self.context, &path, spec),
        }
    }

    fn render_skills(&self, skills: &Skills) -> Result<()> {
        let target = self.out.join(skills.destination);
        match skills.source {
            SkillsSource::RenderClaude => self.generate_skills(Provider::Claude, &target),
            SkillsSource::RenderCodex => self.generate_skills(Provider::Codex, &target),
            SkillsSource::CopyCodex => self.copy_codex_skills(&target),
        }
    }

    fn generate_skills(&self, provider: Provider, target: &Path) -> Result<()> {
        let target = skills::SkillTarget {
            provider,
            path: target,
        };
        skills::render_skills(self.context.source, &[], target)
    }

    fn copy_codex_skills(&self, target: &Path) -> Result<()> {
        let codex = ProviderId::Codex.spec().skills.expect("Codex has skills");
        RealFileSystem.replace_directory(&self.out.join(codex.destination), target)
    }

    fn write_generated_files(&self, spec: &ProviderSpec) -> Result<()> {
        match spec.id {
            ProviderId::Codex => self.write_codex_rules(spec),
            ProviderId::Claude => self.write_claude_policies(spec),
            ProviderId::Devin | ProviderId::Hermes | ProviderId::Pi | ProviderId::Dsh => Ok(()),
        }
    }

    fn write_codex_rules(&self, spec: &ProviderSpec) -> Result<()> {
        match spec.managed_files {
            [rules] => self.write_rules_file(&self.out.join(rules)),
            files => bail!("Codex must declare exactly one rules file, found {files:?}"),
        }
    }

    fn write_rules_file(&self, path: &Path) -> Result<()> {
        command_permissions::write_codex_rules(self.context.source, path)
    }

    fn write_claude_policies(&self, spec: &ProviderSpec) -> Result<()> {
        match spec.generated_files {
            [permissions, protected_paths] => self
                .write_policy_files(&self.out.join(permissions), &self.out.join(protected_paths)),
            files => bail!(
                "Claude must declare command permissions and protected paths, found {files:?}"
            ),
        }
    }

    fn write_policy_files(&self, permissions: &Path, protected_paths: &Path) -> Result<()> {
        command_permissions::write_runtime_policy(self.context.source, permissions)?;
        let paths = protection::protected_paths_for_providers(self.context, self.providers)?;
        protection::write_runtime_policy(paths, protected_paths)
    }

    fn write_managed_config(&self, id: ProviderId, path: &Path) -> Result<()> {
        match id {
            ProviderId::Codex => self.merge_codex_config(path),
            ProviderId::Hermes => hermes_config::write_managed_config(path),
            ProviderId::Claude | ProviderId::Devin | ProviderId::Pi | ProviderId::Dsh => Ok(()),
        }
    }

    fn merge_codex_config(&self, path: &Path) -> Result<()> {
        let generated = codex_config::generated_config_source(self.context)?;
        codex_config::sync_managed_document(&generated, path)
    }
}
