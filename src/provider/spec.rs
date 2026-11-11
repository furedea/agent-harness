#![forbid(clippy::disallowed_methods, clippy::disallowed_types)]

pub(crate) use super::id::ProviderId;
pub(crate) use super::ownership::{
    ConfigFormat, ConfigSync, EntryMatch, KeyScope, Ownership, WhenAbsent, WhenMissing,
};

#[derive(Debug)]
pub(crate) struct Asset {
    pub(crate) destination: &'static str,
    pub(crate) content: &'static str,
}

#[derive(Debug)]
pub(crate) struct SyncedDirectory {
    pub(crate) source: &'static str,
    pub(crate) destination: &'static str,
}

/// Where a provider reads its hook registration from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HookRegistration {
    /// The hooks are a section of the provider's settings file.
    InSettings(&'static str),
    /// The hooks are a file of their own.
    File(&'static str),
}

impl HookRegistration {
    pub(crate) fn path(self) -> &'static str {
        match self {
            Self::InSettings(path) | Self::File(path) => path,
        }
    }
}

/// Fixed files the hook runtime needs inside the provider, embedded in the binary.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Bridge {
    /// Directory published as a whole; assets outside one are published individually.
    pub(crate) directory: Option<&'static str>,
    pub(crate) assets: &'static [Asset],
}

/// How a provider's section of the shared hook config is shaped and merged with external bundles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HookFormat {
    /// The section is the bare event map; a bundle keeps its events under `hooks` in the settings
    /// file.
    SettingsSection,
    /// The section is `{"hooks": events}`; a bundle file also keeps its events under `hooks`.
    WrappedEvents,
    /// The section is `{"hooks": events}`; a bundle file is the event map itself.
    BareEvents,
}

/// Whether the shared hook config must contain a section for the provider.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HookSection {
    Required,
    Optional,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct HookSpec {
    /// Directory of the source tree that holds the hook scripts.
    pub(crate) source: &'static str,
    pub(crate) scripts_dir: &'static str,
    pub(crate) registration: HookRegistration,
    pub(crate) format: HookFormat,
    pub(crate) section: HookSection,
    /// File an external hook bundle writes its registration to.
    pub(crate) external_file: &'static str,
    pub(crate) bridge: Bridge,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SkillSource {
    /// Rendered from the source skills with their frontmatter.
    RenderFrontmatter,
    /// Rendered with frontmatter plus a per-skill invocation policy file.
    RenderWithInvocationPolicy,
    /// Copied from the skills another provider rendered, which that provider must render first.
    CopyRendered(ProviderId),
}

#[derive(Debug)]
pub(crate) struct SkillSpec {
    pub(crate) destination: &'static str,
    pub(crate) source: SkillSource,
}

/// How a provider enforces the command rules, for providers that do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommandRuleSpec {
    /// Rules are a `permissions` block inside the hook registration.
    InHookRegistration,
    /// Rules are files inside the hooks directory.
    RuleFiles {
        permissions: &'static str,
        protected_paths: &'static str,
        /// Files the hooks directory takes from the source and verification requires.
        source_files: &'static [&'static str],
    },
    /// Rules are one execpolicy file published on its own.
    ExecpolicyFile { rules_file: &'static str },
}

impl CommandRuleSpec {
    /// The rules file published individually.
    pub(crate) fn published_file(self) -> Option<&'static str> {
        match self {
            Self::ExecpolicyFile { rules_file } => Some(rules_file),
            Self::InHookRegistration | Self::RuleFiles { .. } => None,
        }
    }

    /// Files rendered inside the hooks directory after it is synced.
    pub(crate) fn generated_files(self) -> Vec<&'static str> {
        match self {
            Self::RuleFiles {
                permissions,
                protected_paths,
                ..
            } => vec![permissions, protected_paths],
            Self::InHookRegistration | Self::ExecpolicyFile { .. } => Vec::new(),
        }
    }

    pub(crate) fn source_files(self) -> &'static [&'static str] {
        match self {
            Self::RuleFiles { source_files, .. } => source_files,
            Self::InHookRegistration | Self::ExecpolicyFile { .. } => &[],
        }
    }
}

/// Where the rendered config the sync reads lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ConfigLocation {
    /// The provider keeps its hook registration and config in one file.
    SameAsHookRegistration,
    File(&'static str),
    /// The config is a directory published as a whole.
    BundleDirectory(&'static str),
}

/// How the rendered config file of a provider is produced at render time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RenderedConfig {
    /// Rendered together with the hook registration or the bridge bundle, so there is nothing
    /// more to write.
    WithHooks,
    /// A fixed document, written as it is.
    Fixed(&'static str),
    /// The source file at this path, applied to the installed config; a missing file adds nothing.
    SourceFile(&'static str),
    /// The source's TOML config with the generated protection and external hook features merged in,
    /// applied to the installed config.
    SourceTomlWithProtection,
}

impl RenderedConfig {
    /// Whether rendering applies the generated config to the installed one, so the installed
    /// config must be carried into the output first.
    pub(crate) fn merges_into_installed(self) -> bool {
        matches!(self, Self::SourceFile(_) | Self::SourceTomlWithProtection)
    }
}

/// The provider's mutable config and how installation syncs the rendered one into it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ConfigSpec {
    pub(crate) location: ConfigLocation,
    pub(crate) rendered: RenderedConfig,
    pub(crate) sync: ConfigSync,
    /// Source directories the config refers to, such as Claude's statusline command.
    pub(crate) directories: &'static [SyncedDirectory],
}

#[derive(Clone, Debug)]
pub(crate) struct ProviderSpec {
    pub(crate) id: ProviderId,
    pub(crate) key: &'static str,
    pub(crate) display_name: &'static str,
    pub(crate) home_dir: &'static str,
    pub(crate) hook: HookSpec,
    pub(crate) skill: Option<&'static SkillSpec>,
    pub(crate) instructions: Option<&'static str>,
    pub(crate) command_rule: Option<CommandRuleSpec>,
    pub(crate) config: Option<ConfigSpec>,
}

impl ProviderSpec {
    /// The rules file published individually.
    pub(crate) fn published_rule_file(&self) -> Option<&'static str> {
        self.command_rule.and_then(CommandRuleSpec::published_file)
    }

    /// Rule files rendered inside the hooks directory after it is synced.
    pub(crate) fn generated_rule_files(&self) -> Vec<&'static str> {
        self.command_rule
            .map_or_else(Vec::new, CommandRuleSpec::generated_files)
    }

    /// Rule files that the hooks directory takes from the source.
    pub(crate) fn source_rule_files(&self) -> &'static [&'static str] {
        self.command_rule.map_or(&[], CommandRuleSpec::source_files)
    }

    /// Source directories the provider's config refers to.
    pub(crate) fn config_directories(&self) -> &'static [SyncedDirectory] {
        self.config.map_or(&[], |config| config.directories)
    }

    /// The rendered config file published on its own, when it is not the hook registration
    /// and not a bundle directory.
    pub(crate) fn config_file(&self) -> Option<&'static str> {
        match self.config?.location {
            ConfigLocation::File(path) => Some(path),
            ConfigLocation::SameAsHookRegistration | ConfigLocation::BundleDirectory(_) => None,
        }
    }

    /// The path of the rendered config, whichever role the file or directory also plays.
    pub(crate) fn config_path(&self) -> Option<&'static str> {
        match self.config?.location {
            ConfigLocation::SameAsHookRegistration => Some(self.hook.registration.path()),
            ConfigLocation::File(path) | ConfigLocation::BundleDirectory(path) => Some(path),
        }
    }
}

pub(crate) use super::registry::PROVIDERS;
