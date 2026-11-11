#![forbid(clippy::disallowed_methods, clippy::disallowed_types)]

use std::path::Path;

use anyhow::{Context, Result};

use crate::{
    config_sync::{
        Merge,
        codec::{ConfigDocument, Toml},
        installed::{Installed, read_optional_generated},
    },
    file_op::FileOp,
    generation::{
        GenerationContext, claude_config, codex_config, command_permissions, hooks, pi_sandbox,
        protection, skills,
    },
    layout::SourceLayout,
    provider::spec::{
        CommandRuleSpec, ConfigSpec, ConfigSync, HookRegistration, ProviderId, ProviderSpec,
        RenderedConfig, SkillSource, SkillSpec,
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
    /// The files a provider declares, as operations to apply in order.
    ///
    /// Operations of one provider may read what an earlier provider's operations wrote, so apply
    /// each provider's before rendering the next.
    pub(crate) fn render(&self, spec: &ProviderSpec) -> Result<Vec<FileOp>> {
        let mut operations = Vec::new();
        operations.extend(self.copy_instructions(spec));
        operations.push(self.write_hook_registration(spec)?);
        operations.extend(self.render_skill(spec)?);
        operations.extend(self.write_command_rule(spec)?);
        operations.extend(self.write_config(spec)?);
        operations.extend(self.write_sandbox_policy(spec)?);
        Ok(operations)
    }

    /// Pi's srt policy, generated from the same source as Claude's sandbox block.
    fn write_sandbox_policy(&self, spec: &ProviderSpec) -> Result<Option<FileOp>> {
        (spec.id == ProviderId::Pi)
            .then(|| pi_sandbox::policy_op(self.context, self.out))
            .transpose()
    }

    fn copy_instructions(&self, spec: &ProviderSpec) -> Option<FileOp> {
        let source = SourceLayout::new(self.context.source.root()).agent_instructions();
        spec.instructions.map(|path| FileOp::CopyFile {
            source,
            target: self.out.join(path),
        })
    }

    fn write_hook_registration(&self, spec: &ProviderSpec) -> Result<FileOp> {
        let path = self.out.join(spec.hook.registration.path());
        match spec.hook.registration {
            HookRegistration::InSettings(_) => self.write_settings(&path),
            HookRegistration::File(_) => hooks::hooks_op_for_runtime(self.context, &path, spec),
        }
    }

    fn write_settings(&self, path: &Path) -> Result<FileOp> {
        claude_config::settings_op_for_runtime(self.context, path)
    }

    fn render_skill(&self, spec: &ProviderSpec) -> Result<Option<FileOp>> {
        spec.skill
            .map(|skill| self.render_declared_skill(skill))
            .transpose()
    }

    fn render_declared_skill(&self, skill: &SkillSpec) -> Result<FileOp> {
        let target = self.out.join(skill.destination);
        let render = |provider| self.generate_skills(provider, &target);
        match skill.source {
            SkillSource::RenderFrontmatter => render(Provider::Claude),
            SkillSource::RenderWithInvocationPolicy => render(Provider::Codex),
            SkillSource::CopyRendered(from) => self.copy_rendered_skills(from, target),
        }
    }

    fn generate_skills(&self, provider: Provider, target: &Path) -> Result<FileOp> {
        let target = skills::SkillTarget {
            provider,
            path: target,
        };
        skills::render_skills(self.context.source, &[], target)
    }

    fn copy_rendered_skills(&self, from: ProviderId, target: std::path::PathBuf) -> Result<FileOp> {
        let rendered = from
            .spec()
            .skill
            .with_context(|| format!("the {} provider spec declares no skills", from.spec().key))?;
        Ok(FileOp::CopyTree {
            source: self.out.join(rendered.destination),
            target,
        })
    }

    fn write_command_rule(&self, spec: &ProviderSpec) -> Result<Vec<FileOp>> {
        match spec.command_rule {
            // Devin's rules are written into its hook registration.
            None | Some(CommandRuleSpec::InHookRegistration) => Ok(Vec::new()),
            Some(CommandRuleSpec::RuleFiles {
                permissions,
                protected_paths,
                ..
            }) => self
                .write_policy_files(&self.out.join(permissions), &self.out.join(protected_paths)),
            Some(CommandRuleSpec::ExecpolicyFile { rules_file }) => self
                .write_rules_file(rules_file)
                .map(|operation| vec![operation]),
        }
    }

    fn write_rules_file(&self, rules_file: &str) -> Result<FileOp> {
        let path = self.out.join(rules_file);
        command_permissions::codex_rules_op(self.context.source, &path)
    }

    fn write_policy_files(
        &self,
        permissions: &Path,
        protected_paths: &Path,
    ) -> Result<Vec<FileOp>> {
        let paths = protection::protected_paths_for_providers(self.context, self.providers)?;
        Ok(vec![
            command_permissions::runtime_policy_op(self.context.source, permissions)?,
            protection::runtime_policy_op(paths, protected_paths)?,
        ])
    }

    fn write_config(&self, spec: &ProviderSpec) -> Result<Vec<FileOp>> {
        match spec.config.zip(spec.config_file()) {
            Some((config, path)) => self.write_config_file(config, &self.out.join(path)),
            None => Ok(Vec::new()),
        }
    }

    fn write_config_file(&self, config: ConfigSpec, path: &Path) -> Result<Vec<FileOp>> {
        let target = InstalledConfig {
            sync: &config.sync,
            path,
        };
        match config.rendered {
            // The hook registration or the bridge directory, rendered with them.
            RenderedConfig::WithHooks => Ok(Vec::new()),
            RenderedConfig::Fixed(content) => Ok(vec![FileOp::write(path, content)]),
            RenderedConfig::SourceFile(relative) => self.merge_source_file(target, relative),
            RenderedConfig::SourceTomlWithProtection => self.merge_protected_toml(target),
        }
    }

    fn merge_source_file(
        &self,
        target: InstalledConfig<'_>,
        relative: &str,
    ) -> Result<Vec<FileOp>> {
        let path = self.context.source.root().join(relative);
        let generated = read_optional_generated(target.sync, self.context.source.files(), &path)?;
        self.merge_into_installed(target, &generated)
    }

    fn merge_protected_toml(&self, target: InstalledConfig<'_>) -> Result<Vec<FileOp>> {
        let generated = codex_config::generated_config_source(self.context)?;
        self.merge_into_installed(
            target,
            &ConfigDocument::Toml(Toml::from_document(generated)),
        )
    }

    fn merge_into_installed(
        &self,
        target: InstalledConfig<'_>,
        generated: &ConfigDocument,
    ) -> Result<Vec<FileOp>> {
        let installed =
            Installed::from_snapshot(target.sync, self.context.source.files(), target.path)?;
        Merge::new(target.sync, generated)
            .apply(installed)?
            .into_ops(target.sync, target.path)
    }
}

/// A mutable config in the output, which rendering merges the generated config into.
#[derive(Clone, Copy)]
struct InstalledConfig<'a> {
    sync: &'a ConfigSync,
    path: &'a Path,
}
