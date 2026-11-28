#![forbid(clippy::disallowed_methods, clippy::disallowed_types)]

use std::path::Path;

use anyhow::{Context, Result, bail, ensure};
use serde_json::json;

use crate::{
    config_sync::{
        Merge,
        codec::{ConfigDocument, Toml},
        installed::{InstalledConfigs, parse_generated},
    },
    file_op::FileOp,
    generation::{
        GenerationContext, antigravity_config, claude_config, codex_config, command_permissions,
        hooks, skills,
    },
    layout::SourceLayout,
    provider::spec::{
        CommandRuleSpec, ConfigSpec, ConfigSync, HookRegistration, McpEntry, McpMapping, McpSpec,
        ProviderId, ProviderSpec, RenderedConfig, SkillSource, SkillSpec,
    },
    render::Provider,
    source_model::Source,
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
    /// The home directory when `out` is installed into it.
    pub(crate) home: Option<&'a Path>,
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
        operations.extend(self.write_mcp_file(spec)?);
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
            .map(|skill| self.out.join(skill.destination))
            .map(|path| SkillsDirectory::of(spec, &path).render(self.context.source))
            .transpose()
    }

    fn write_command_rule(&self, spec: &ProviderSpec) -> Result<Vec<FileOp>> {
        match spec.command_rule {
            // Devin's rules are written into its hook registration.
            None | Some(CommandRuleSpec::InHookRegistration) => Ok(Vec::new()),
            Some(CommandRuleSpec::RuleFiles { permissions, .. }) => self
                .write_runtime_permissions(permissions)
                .map(|operation| vec![operation]),
            Some(CommandRuleSpec::ExecpolicyFile { rules_file }) => self
                .write_rules_file(rules_file)
                .map(|operation| vec![operation]),
        }
    }

    fn write_runtime_permissions(&self, permissions: &str) -> Result<FileOp> {
        let path = self.out.join(permissions);
        command_permissions::runtime_policy_op(self.context.source, &path)
    }

    fn write_rules_file(&self, rules_file: &str) -> Result<FileOp> {
        let path = self.out.join(rules_file);
        command_permissions::codex_rules_op(self.context.source, &path)
    }

    fn write_config(&self, spec: &ProviderSpec) -> Result<Vec<FileOp>> {
        match spec.config.zip(spec.config_file()) {
            Some((config, relative)) => self.write_config_file(ConfigFile {
                config,
                relative,
                mcp: spec.mcp_mapping(),
            }),
            None => Ok(Vec::new()),
        }
    }

    fn write_mcp_file(&self, spec: &ProviderSpec) -> Result<Vec<FileOp>> {
        match spec.mcp_file() {
            Some((config, relative)) => self.write_config_file(ConfigFile {
                config,
                relative,
                mcp: None,
            }),
            None => Ok(Vec::new()),
        }
    }

    fn write_config_file(&self, file: ConfigFile) -> Result<Vec<FileOp>> {
        let target = InstalledConfig {
            sync: &file.config.sync,
            relative: file.relative,
            path: &self.out.join(file.relative),
        };
        match file.config.rendered {
            // The hook registration or the bridge directory, rendered with them.
            RenderedConfig::WithHooks => Ok(Vec::new()),
            RenderedConfig::Fixed(content) => self.write_fixed(file, content),
            RenderedConfig::SourceFile(relative) => self.merge_source_file(target, relative),
            RenderedConfig::SourceTomlWithGuardedFilesystem => self.merge_guarded_toml(target),
            RenderedConfig::SourceJsonWithToolPermissions(p) => self.merge_tool_rules(target, p),
            RenderedConfig::McpServers(mcp) => self.merge_mcp_servers(target, mcp),
        }
    }

    /// The fixed document, with the source's MCP servers when the provider reads them from it.
    fn write_fixed(&self, file: ConfigFile, content: &str) -> Result<Vec<FileOp>> {
        let path = self.out.join(file.relative);
        let sync = &file.config.sync;
        let text = match file
            .mcp
            .filter(|_| !self.context.source.mcp_servers.is_empty())
        {
            Some(mcp) => self.with_mcp_servers(parse_generated(sync, &path, content)?, mcp)?,
            None => content.to_owned(),
        };
        Ok(vec![FileOp::write(&path, text)])
    }

    fn with_mcp_servers(&self, mut document: ConfigDocument, mcp: McpMapping) -> Result<String> {
        self.set_mcp_servers(&mut document, mcp)?;
        document.render()
    }

    fn merge_mcp_servers(
        &self,
        target: InstalledConfig<'_>,
        mcp: McpMapping,
    ) -> Result<Vec<FileOp>> {
        let mut generated = ConfigDocument::blank(target.sync.format);
        self.set_mcp_servers(&mut generated, mcp)?;
        self.merge_into_installed(target, &generated)
    }

    fn set_mcp_servers(&self, document: &mut ConfigDocument, mcp: McpMapping) -> Result<()> {
        let servers = self.context.source.mcp_servers.json_map(mcp.entry);
        document
            .set_json(mcp.path, servers)
            .context("failed to render the MCP servers")
    }

    fn merge_source_file(
        &self,
        target: InstalledConfig<'_>,
        source_relative: &str,
    ) -> Result<Vec<FileOp>> {
        let generated = self.source_config(source_relative)?;
        self.merge_into_installed(target, generated)
    }

    fn source_config(&self, source_relative: &str) -> Result<&ConfigDocument> {
        self.context
            .source
            .provider_configs
            .get(source_relative)
            .with_context(|| format!("source config {source_relative} was not loaded"))
    }

    fn merge_guarded_toml(&self, target: InstalledConfig<'_>) -> Result<Vec<FileOp>> {
        let generated = codex_config::generated_config_source(self.context)?;
        self.merge_into_installed(
            target,
            &ConfigDocument::Toml(Toml::from_document(generated)),
        )
    }

    fn merge_tool_rules(
        &self,
        target: InstalledConfig<'_>,
        source_relative: &str,
    ) -> Result<Vec<FileOp>> {
        let source = self
            .source_config(source_relative)?
            .as_json()
            .with_context(|| format!("source config {source_relative} is not JSON"))?;
        let generated = antigravity_config::settings_for_runtime(self.context, source)?;
        self.merge_into_installed(target, &ConfigDocument::Json(generated))
    }

    fn merging<'m>(&self, merge: Merge<'m>) -> Merge<'m>
    where
        Self: 'm,
    {
        match self.home {
            Some(home) => merge.expanding_home(home),
            None => merge,
        }
    }

    fn merge_into_installed(
        &self,
        target: InstalledConfig<'_>,
        generated: &ConfigDocument,
    ) -> Result<Vec<FileOp>> {
        self.merging(Merge::new(target.sync, generated))
            .apply(self.installed.get(target.relative))?
            .into_ops(target.sync, target.path)
    }
}

/// A provider's skills directory and where it is rendered.
#[derive(Clone, Copy)]
pub(crate) struct SkillsDirectory<'a> {
    pub(crate) provider: &'a ProviderSpec,
    pub(crate) path: &'a Path,
}

impl<'a> SkillsDirectory<'a> {
    pub(crate) fn of(provider: &'a ProviderSpec, path: &'a Path) -> Self {
        Self { provider, path }
    }

    /// The provider's skills, with its MCP plugin when it reads the servers from one.
    ///
    /// # Errors
    ///
    /// Returns an error when the provider has no skills, a skill cannot be rendered, or a skill
    /// takes the plugin's name.
    pub(crate) fn render(self, source: &Source) -> Result<FileOp> {
        let spec = self.provider;
        let skill = spec.skill.with_context(|| no_skills("render", spec.key))?;
        let skills = render_declared_skill(source, skill, self.path)?;
        add_mcp_plugin(source, self, skills)
    }
}

fn render_declared_skill(source: &Source, skill: &SkillSpec, target: &Path) -> Result<FileOp> {
    match skill.source {
        SkillSource::RenderFrontmatter => skills_for(source, Provider::Claude, target),
        SkillSource::RenderWithInvocationPolicy => skills_for(source, Provider::Codex, target),
        SkillSource::CopyRendered(from) => reuse_rendered_skills(source, from, target),
    }
}

/// Another provider's rendered skills, moved to `target` from where that provider puts them.
fn reuse_rendered_skills(source: &Source, from: ProviderId, target: &Path) -> Result<FileOp> {
    let rendered = from
        .spec()
        .skill
        .with_context(|| no_skills("copy", from.spec().key))?;
    let origin = Path::new(rendered.destination);
    Ok(render_declared_skill(source, rendered, origin)?.relocated(origin, target))
}

fn no_skills(operation: &str, provider: &str) -> String {
    format!("failed to {operation} the {provider} skills: the provider spec declares none")
}

fn skills_for(source: &Source, provider: Provider, target: &Path) -> Result<FileOp> {
    skills::render_skills(
        source,
        skills::SkillTarget {
            provider,
            path: target,
        },
    )
}

fn add_mcp_plugin(
    source: &Source,
    directory: SkillsDirectory<'_>,
    skills: FileOp,
) -> Result<FileOp> {
    match directory.provider.mcp {
        Some(McpSpec::SkillsDirectoryPlugin { directory: plugin }) => {
            with_mcp_plugin(source, skills, &directory.path.join(plugin))
        }
        Some(McpSpec::ConfigMapping { .. } | McpSpec::ConfigFile(_)) | None => Ok(skills),
    }
}

/// The skills tree with a plugin at `plugin` that declares the source's MCP servers; unchanged
/// when the source declares none.
fn with_mcp_plugin(source: &Source, skills: FileOp, plugin: &Path) -> Result<FileOp> {
    if source.mcp_servers.is_empty() {
        return Ok(skills);
    }
    let FileOp::ReplaceTree {
        target,
        mut contents,
    } = skills
    else {
        bail!("failed to add the MCP server plugin: rendered skills must replace their directory");
    };
    ensure!(
        !writes_under(&contents, plugin),
        "failed to add the MCP server plugin at {}: a skill already uses that directory",
        plugin.display()
    );
    let manifest = json!({
        "name": MCP_PLUGIN_NAME,
        "description": "MCP servers declared in the agent-harness source",
        "mcpServers": source.mcp_servers.json_map(McpEntry::TypedUrl),
    });
    contents.push(FileOp::write(
        plugin.join(".claude-plugin/plugin.json"),
        serde_json::to_string_pretty(&manifest)
            .context("failed to serialize the MCP server plugin manifest")?
            + "\n",
    ));
    Ok(FileOp::ReplaceTree { target, contents })
}

fn writes_under(operations: &[FileOp], directory: &Path) -> bool {
    operations
        .iter()
        .any(|operation| operation.target().starts_with(directory))
}

/// The plugin name, which prefixes the tool names Claude Code gives the servers'
/// tools: `mcp__plugin_agent-harness_<server>__<tool>`.
const MCP_PLUGIN_NAME: &str = "agent-harness";

/// A config file a provider declares, to render into the output.
#[derive(Clone, Copy)]
struct ConfigFile {
    config: ConfigSpec,
    /// The registry's path of the file.
    relative: &'static str,
    /// Where the provider reads the MCP servers inside the file, when it does.
    mcp: Option<McpMapping>,
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
