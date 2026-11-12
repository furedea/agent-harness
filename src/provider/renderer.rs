#![forbid(clippy::disallowed_methods, clippy::disallowed_types)]

use std::path::Path;

use anyhow::{Context, Result};

use crate::{
    config_sync::{
        Merge,
        codec::{ConfigDocument, Toml},
        installed::{InstalledConfigs, parse_generated},
    },
    file_op::FileOp,
    generation::{
        GenerationContext, claude_config, codex_config, command_permissions, hooks, protection,
        skills,
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
///
/// A provider that reuses another's output (copied skills) computes it again from the same
/// `Source`, so no provider reads what an earlier one wrote and the whole installation is one plan.
pub(crate) struct ProviderRenderer<'a> {
    pub(crate) context: GenerationContext<'a>,
    pub(crate) providers: &'a [ProviderSpec],
    pub(crate) installed: &'a InstalledConfigs,
    pub(crate) out: &'a Path,
}

impl ProviderRenderer<'_> {
    /// The files a provider declares, as operations to apply in order.
    pub(crate) fn render(&self, spec: &ProviderSpec) -> Result<Vec<FileOp>> {
        let mut operations = Vec::new();
        operations.extend(self.copy_instructions(spec));
        operations.push(self.write_hook_registration(spec)?);
        operations.extend(self.render_skill(spec)?);
        operations.extend(self.write_command_rule(spec)?);
        operations.extend(self.write_config(spec)?);
        Ok(operations)
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
            HookRegistration::InSettings(_) => self.write_settings(spec, &path),
            HookRegistration::File(_) => hooks::hooks_op_for_runtime(self.context, &path, spec),
        }
    }

    /// The settings with the installed ones merged in, when the provider's config claims the
    /// whole settings file.
    ///
    /// `~/` stays unexpanded: the prefix is not always the home, and the Home Manager sync expands
    /// it.
    fn write_settings(&self, spec: &ProviderSpec, path: &Path) -> Result<FileOp> {
        match spec
            .config
            .filter(|config| config.sync.replaces_every_key())
        {
            Some(config) => self.merge_settings_into_installed(InstalledConfig {
                sync: &config.sync,
                relative: spec.hook.registration.path(),
                path,
            }),
            None => claude_config::settings_op_for_runtime(self.context, path),
        }
    }

    fn merge_settings_into_installed(&self, target: InstalledConfig<'_>) -> Result<FileOp> {
        let settings = claude_config::settings_for_runtime(self.context)?;
        let text = serde_json::to_string_pretty(&settings)? + "\n";
        let generated = parse_generated(target.sync, target.path, &text)?;
        let installed = self.installed.get(target.relative);
        let merged = Merge::new(target.sync, &generated).apply(installed)?;
        Ok(FileOp::write(target.path, merged.config))
    }

    fn render_skill(&self, spec: &ProviderSpec) -> Result<Option<FileOp>> {
        spec.skill
            .map(|skill| self.render_declared_skill(skill, &self.out.join(skill.destination)))
            .transpose()
    }

    fn render_declared_skill(&self, skill: &SkillSpec, target: &Path) -> Result<FileOp> {
        match skill.source {
            SkillSource::RenderFrontmatter => self.skills_for(Provider::Claude, target),
            SkillSource::RenderWithInvocationPolicy => self.skills_for(Provider::Codex, target),
            SkillSource::CopyRendered(from) => self.reuse_rendered_skills(from, target),
        }
    }

    /// Another provider's rendered skills, moved to `target` from where that provider puts them.
    fn reuse_rendered_skills(&self, from: ProviderId, target: &Path) -> Result<FileOp> {
        let rendered = from
            .spec()
            .skill
            .with_context(|| format!("the {} provider spec declares no skills", from.spec().key))?;
        let origin = self.out.join(rendered.destination);
        Ok(self
            .render_declared_skill(rendered, &origin)?
            .relocated(&origin, target))
    }

    fn skills_for(&self, provider: Provider, target: &Path) -> Result<FileOp> {
        let target = skills::SkillTarget {
            provider,
            path: target,
        };
        skills::render_skills(self.context.source, target)
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
            Some((config, path)) => self.write_config_file(config, path),
            None => Ok(Vec::new()),
        }
    }

    fn write_config_file(&self, config: ConfigSpec, relative: &str) -> Result<Vec<FileOp>> {
        let target = InstalledConfig {
            sync: &config.sync,
            relative,
            path: &self.out.join(relative),
        };
        match config.rendered {
            // The hook registration or the bridge directory, rendered with them.
            RenderedConfig::WithHooks => Ok(Vec::new()),
            RenderedConfig::Fixed(content) => Ok(vec![FileOp::write(target.path, content)]),
            RenderedConfig::SourceFile(relative) => self.merge_source_file(target, relative),
            RenderedConfig::SourceTomlWithProtection => self.merge_protected_toml(target),
        }
    }

    fn merge_source_file(
        &self,
        target: InstalledConfig<'_>,
        source_relative: &str,
    ) -> Result<Vec<FileOp>> {
        let generated = self
            .context
            .source
            .provider_configs
            .get(source_relative)
            .with_context(|| format!("source config {source_relative} was not loaded"))?;
        self.merge_into_installed(target, generated)
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
        Merge::new(target.sync, generated)
            .apply(self.installed.get(target.relative))?
            .into_ops(target.sync, target.path)
    }
}

/// A mutable config in the output, which rendering merges the generated config into.
#[derive(Clone, Copy)]
struct InstalledConfig<'a> {
    sync: &'a ConfigSync,
    /// The registry's path of the config, which the installed configs are keyed by.
    relative: &'a str,
    /// The config in the output.
    path: &'a Path,
}
