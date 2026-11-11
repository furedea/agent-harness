use std::path::Path;

use anyhow::{Context, Result};

use crate::{
    filesystem::{FileSystem, RealFileSystem},
    generation::{
        GenerationContext, claude_config, codex_config, command_permissions, hermes_config, hooks,
        pi_config, protection, skills,
    },
    layout::SourceLayout,
    provider::spec::{
        CommandRuleSpec, ConfigSync, HookRegistration, ProviderId, ProviderSpec, SkillSource,
        SkillSpec,
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
        spec.skill
            .map(|skill| self.render_skill(skill))
            .transpose()?;
        spec.command_rule
            .map(|rule| self.write_command_rule(rule))
            .transpose()?;
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
        let path = self.out.join(spec.hook.registration.path());
        match spec.hook.registration {
            HookRegistration::InSettings(_) => self.write_settings(&path),
            HookRegistration::File(_) => hooks::write_hooks_for_runtime(self.context, &path, spec),
        }
    }

    fn write_settings(&self, path: &Path) -> Result<()> {
        claude_config::write_settings_for_runtime(self.context, path)
    }

    fn render_skill(&self, skill: &SkillSpec) -> Result<()> {
        let target = self.out.join(skill.destination);
        let render = |provider| self.generate_skills(provider, &target);
        match skill.source {
            SkillSource::RenderFrontmatter => render(Provider::Claude),
            SkillSource::RenderWithInvocationPolicy => render(Provider::Codex),
            SkillSource::CopyRendered(from) => self.copy_rendered_skills(from, &target),
        }
    }

    fn generate_skills(&self, provider: Provider, target: &Path) -> Result<()> {
        let target = skills::SkillTarget {
            provider,
            path: target,
        };
        skills::render_skills(self.context.source, &[], target)
    }

    fn copy_rendered_skills(&self, from: ProviderId, target: &Path) -> Result<()> {
        let rendered = from
            .spec()
            .skill
            .with_context(|| format!("the {} provider spec declares no skills", from.spec().key))?;
        RealFileSystem.replace_directory(&self.out.join(rendered.destination), target)
    }

    fn write_command_rule(&self, rule: CommandRuleSpec) -> Result<()> {
        match rule {
            // Devin's rules are written into its hook registration.
            CommandRuleSpec::InHookRegistration => Ok(()),
            CommandRuleSpec::RuleFiles {
                permissions,
                protected_paths,
                ..
            } => self
                .write_policy_files(&self.out.join(permissions), &self.out.join(protected_paths)),
            CommandRuleSpec::ExecpolicyFile { rules_file } => self.write_rules_file(rules_file),
        }
    }

    fn write_rules_file(&self, rules_file: &str) -> Result<()> {
        let path = self.out.join(rules_file);
        command_permissions::write_codex_rules(self.context.source, &path)
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
        let source = self.context.source;
        match sync {
            ConfigSync::MergeToml => self.merge_codex_config(path),
            ConfigSync::MergeYaml => hermes_config::write_managed_config(path),
            ConfigSync::ReplaceListedKeys => pi_config::write_managed_config(source, path),
            // These are the hook registration or the bridge directory, rendered with them.
            ConfigSync::ReplaceGeneratedKeys
            | ConfigSync::MergePermissionEntries
            | ConfigSync::RegisterBundle => Ok(()),
        }
    }

    fn merge_codex_config(&self, path: &Path) -> Result<()> {
        let generated = codex_config::generated_config_source(self.context)?;
        codex_config::sync_managed_document(&generated, path)
    }
}
