use std::path::Path;

use anyhow::Result;

use crate::{
    filesystem::{FileSystem, RealFileSystem},
    generation::{
        GenerationContext, claude_config, codex_config, command_permissions, hermes_config, hooks,
        protection, skills,
    },
    layout::SourceLayout,
    provider::spec::{
        CommandRules, ConfigSync, HookRegistration, ProviderSpec, Skills, SkillsSource,
    },
    render::Provider,
};

/// Provider functions: what each provider's declared files contain.
///
/// The registry declares which files a provider owns and which kind of each it is; this renders
/// them. Every `match` is exhaustive over a registry enum, so adding a variant fails to compile
/// until it is rendered.
pub(crate) struct ProviderRenderer<'a> {
    pub(crate) context: GenerationContext<'a>,
    pub(crate) providers: &'a [ProviderSpec],
    pub(crate) out: &'a Path,
}

impl ProviderRenderer<'_> {
    pub(crate) fn render(&self, spec: &ProviderSpec) -> Result<()> {
        self.copy_instructions(spec)?;
        self.write_hook_registration(spec)?;
        spec.skills
            .map(|skills| self.render_skills(skills))
            .transpose()?;
        self.write_command_rules(spec.command_rules)?;
        self.write_config(spec)
    }

    fn copy_instructions(&self, spec: &ProviderSpec) -> Result<()> {
        let source = SourceLayout::new(self.context.source).agent_instructions();
        spec.instructions
            .map(|path| RealFileSystem.copy_file(&source, &self.out.join(path)))
            .transpose()?;
        Ok(())
    }

    fn write_hook_registration(&self, spec: &ProviderSpec) -> Result<()> {
        let path = self.out.join(spec.hooks.registration.path());
        match spec.hooks.registration {
            HookRegistration::InSettings(_) => self.write_settings(&path),
            HookRegistration::File(_) => hooks::write_hooks_for_runtime(self.context, &path, spec),
        }
    }

    fn write_settings(&self, path: &Path) -> Result<()> {
        claude_config::write_settings_for_runtime(self.context, path)
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
        let codex = crate::provider::spec::ProviderId::Codex
            .spec()
            .skills
            .expect("Codex has skills");
        RealFileSystem.replace_directory(&self.out.join(codex.destination), target)
    }

    fn write_command_rules(&self, rules: CommandRules) -> Result<()> {
        match rules {
            // Devin's rules are written into its hook registration.
            CommandRules::None | CommandRules::DevinHookBlock => Ok(()),
            CommandRules::ClaudeRules {
                permissions,
                protected_paths,
                ..
            } => self
                .write_policy_files(&self.out.join(permissions), &self.out.join(protected_paths)),
            CommandRules::CodexExecpolicy { rules_file } => command_permissions::write_codex_rules(
                self.context.source,
                &self.out.join(rules_file),
            ),
        }
    }

    fn write_policy_files(&self, permissions: &Path, protected_paths: &Path) -> Result<()> {
        command_permissions::write_runtime_policy(self.context.source, permissions)?;
        let paths = protection::protected_paths_for_providers(self.context, self.providers)?;
        protection::write_runtime_policy(paths, protected_paths)
    }

    fn write_config(&self, spec: &ProviderSpec) -> Result<()> {
        spec.config
            .zip(spec.config_file())
            .map(|(config, path)| self.write_config_file(config.sync, &self.out.join(path)))
            .transpose()?;
        Ok(())
    }

    fn write_config_file(&self, sync: ConfigSync, path: &Path) -> Result<()> {
        match sync {
            ConfigSync::MergeToml => self.merge_codex_config(path),
            ConfigSync::MergeYaml => hermes_config::write_managed_config(path),
            // These are the hook registration or the bridge directory, rendered with them.
            ConfigSync::MergeClaudeSettings
            | ConfigSync::MergeTopLevelJson
            | ConfigSync::RegisterBundle => Ok(()),
        }
    }

    fn merge_codex_config(&self, path: &Path) -> Result<()> {
        let generated = codex_config::generated_config_source(self.context)?;
        codex_config::sync_managed_document(&generated, path)
    }
}
