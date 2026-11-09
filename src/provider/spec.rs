/// How installation keeps a provider's mutable config in step with the rendered one.
///
/// A variant names the managed operation; `config_sync` owns the code that performs it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ConfigSync {
    MergeClaudeSettings,
    MergeToml,
    MergeTopLevelJson,
    MergeYaml,
    RegisterBundle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, clap::ValueEnum)]
pub(crate) enum ProviderId {
    Claude,
    Codex,
    Devin,
    Hermes,
    Pi,
    Dsh,
}

impl ProviderId {
    pub(crate) fn spec(self) -> &'static ProviderSpec {
        PROVIDERS
            .iter()
            .find(|spec| spec.id == self)
            .expect("registered provider")
    }
}

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

#[derive(Clone, Copy, Debug)]
pub(crate) struct Hooks {
    /// Directory of the source tree that holds the hook scripts.
    pub(crate) source: &'static str,
    pub(crate) scripts_dir: &'static str,
    pub(crate) registration: HookRegistration,
    /// File an external hook bundle writes its registration to.
    pub(crate) external_file: &'static str,
    pub(crate) bridge: Bridge,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SkillsSource {
    RenderClaude,
    RenderCodex,
    CopyCodex,
}

#[derive(Debug)]
pub(crate) struct Skills {
    pub(crate) destination: &'static str,
    pub(crate) source: SkillsSource,
}

/// How a provider enforces the command rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommandRules {
    None,
    /// Rules are a `permissions` block inside the hook registration.
    DevinHookBlock,
    /// Rules are files inside the hooks directory.
    ClaudeRules {
        permissions: &'static str,
        protected_paths: &'static str,
        /// Files the hooks directory takes from the source and verification requires.
        source_files: &'static [&'static str],
    },
    /// Rules are one execpolicy file published on its own.
    CodexExecpolicy {
        rules_file: &'static str,
    },
}

impl CommandRules {
    /// The rules file published individually.
    pub(crate) fn published_file(self) -> Option<&'static str> {
        match self {
            Self::CodexExecpolicy { rules_file } => Some(rules_file),
            Self::None | Self::DevinHookBlock | Self::ClaudeRules { .. } => None,
        }
    }

    /// Files rendered inside the hooks directory after it is synced.
    pub(crate) fn generated_files(self) -> Vec<&'static str> {
        match self {
            Self::ClaudeRules {
                permissions,
                protected_paths,
                ..
            } => vec![permissions, protected_paths],
            Self::None | Self::DevinHookBlock | Self::CodexExecpolicy { .. } => Vec::new(),
        }
    }

    pub(crate) fn source_files(self) -> &'static [&'static str] {
        match self {
            Self::ClaudeRules { source_files, .. } => source_files,
            Self::None | Self::DevinHookBlock | Self::CodexExecpolicy { .. } => &[],
        }
    }
}

/// The provider's mutable config and how installation syncs the rendered one into it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Config {
    /// The rendered config the sync reads: a file, or the bundle directory for `RegisterBundle`.
    /// It may be the hook registration itself when the provider keeps both in one file.
    pub(crate) path: &'static str,
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
    pub(crate) hooks: Hooks,
    pub(crate) skills: Option<&'static Skills>,
    pub(crate) instructions: Option<&'static str>,
    pub(crate) command_rules: CommandRules,
    pub(crate) config: Option<Config>,
}

impl ProviderSpec {
    /// Source directories the provider's config refers to.
    pub(crate) fn config_directories(&self) -> &'static [SyncedDirectory] {
        self.config.map_or(&[], |config| config.directories)
    }

    /// The rendered config file published on its own, when it is not the hook registration
    /// and not a bundle directory.
    pub(crate) fn config_file(&self) -> Option<&'static str> {
        self.config
            .filter(|config| config.sync != ConfigSync::RegisterBundle)
            .map(|config| config.path)
            .filter(|path| *path != self.hooks.registration.path())
    }
}

pub(crate) const PROVIDERS: &[ProviderSpec] = &[
    ProviderSpec {
        id: ProviderId::Codex,
        key: "codex",
        display_name: "Codex",
        home_dir: ".codex",
        hooks: Hooks {
            source: "codex/hooks",
            scripts_dir: ".codex/hooks",
            registration: HookRegistration::File(".codex/hooks.json"),
            external_file: ".codex/hooks.json",
            bridge: Bridge {
                directory: None,
                assets: &[],
            },
        },
        skills: Some(&Skills {
            destination: ".codex/skills",
            source: SkillsSource::RenderCodex,
        }),
        instructions: Some(".codex/AGENTS.md"),
        command_rules: CommandRules::CodexExecpolicy {
            rules_file: ".codex/rules/default.rules",
        },
        config: Some(Config {
            path: ".codex/config.toml",
            sync: ConfigSync::MergeToml,
            directories: &[],
        }),
    },
    ProviderSpec {
        id: ProviderId::Claude,
        key: "claude",
        display_name: "Claude",
        home_dir: ".claude",
        hooks: Hooks {
            source: "hooks",
            scripts_dir: ".claude/hooks",
            registration: HookRegistration::InSettings(".claude/settings.json"),
            external_file: ".claude/settings.json",
            bridge: Bridge {
                directory: None,
                assets: &[],
            },
        },
        skills: Some(&Skills {
            destination: ".claude/skills",
            source: SkillsSource::RenderClaude,
        }),
        instructions: Some(".claude/CLAUDE.md"),
        command_rules: CommandRules::ClaudeRules {
            permissions: ".claude/hooks/rules/command_permissions.json",
            protected_paths: ".claude/hooks/rules/protected_paths.json",
            source_files: &[
                ".claude/hooks/rules/allowed_commands.json",
                ".claude/hooks/rules/forbidden_commands.json",
                ".claude/hooks/rules/secret_commit_policy.json",
                ".claude/hooks/rules/secret_path_policy.json",
            ],
        },
        // settings.json is also the hook registration: one file, two roles.
        config: Some(Config {
            path: ".claude/settings.json",
            sync: ConfigSync::MergeClaudeSettings,
            directories: &[SyncedDirectory {
                source: "claude/statusline",
                destination: ".claude/statusline",
            }],
        }),
    },
    ProviderSpec {
        id: ProviderId::Devin,
        key: "devin",
        display_name: "Devin",
        home_dir: ".devin",
        hooks: Hooks {
            source: "devin/hooks",
            scripts_dir: ".devin/hooks",
            registration: HookRegistration::File(".config/devin/config.json"),
            external_file: ".devin/hooks.v1.json",
            bridge: Bridge {
                directory: None,
                assets: &[],
            },
        },
        skills: None,
        instructions: None,
        command_rules: CommandRules::DevinHookBlock,
        // config.json is also the hook registration: one file, two roles.
        config: Some(Config {
            path: ".config/devin/config.json",
            sync: ConfigSync::MergeTopLevelJson,
            directories: &[],
        }),
    },
    ProviderSpec {
        id: ProviderId::Hermes,
        key: "hermes",
        display_name: "Hermes",
        home_dir: ".hermes",
        hooks: Hooks {
            source: "hermes/hooks",
            scripts_dir: ".hermes/hooks",
            registration: HookRegistration::File(".hermes/hooks.json"),
            external_file: ".hermes/hooks.json",
            bridge: Bridge {
                directory: Some(".hermes/plugins/agent-harness-hooks"),
                assets: &[
                    Asset {
                        destination: ".hermes/plugins/agent-harness-hooks/plugin.yaml",
                        content: include_str!("../generation/assets/hermes/plugin.yaml"),
                    },
                    Asset {
                        destination: ".hermes/plugins/agent-harness-hooks/__init__.py",
                        content: include_str!("../generation/assets/hermes/__init__.py"),
                    },
                ],
            },
        },
        skills: None,
        instructions: None,
        command_rules: CommandRules::None,
        config: Some(Config {
            path: ".hermes/managed-config.yaml",
            sync: ConfigSync::MergeYaml,
            directories: &[],
        }),
    },
    ProviderSpec {
        id: ProviderId::Pi,
        key: "pi",
        display_name: "Pi",
        home_dir: ".pi",
        hooks: Hooks {
            source: "pi/hooks",
            scripts_dir: ".pi/hooks",
            registration: HookRegistration::File(".pi/agent/hooks.json"),
            external_file: ".pi/hooks.json",
            bridge: Bridge {
                directory: None,
                assets: &[Asset {
                    destination: ".pi/agent/extensions/hook_bridge.ts",
                    content: include_str!("../generation/assets/pi/hook_bridge.ts"),
                }],
            },
        },
        skills: None,
        instructions: None,
        command_rules: CommandRules::None,
        config: None,
    },
    ProviderSpec {
        id: ProviderId::Dsh,
        key: "dsh",
        display_name: "Dsh",
        home_dir: ".dsh",
        hooks: Hooks {
            source: "dsh/hooks",
            scripts_dir: ".dsh/hooks",
            registration: HookRegistration::File(".dsh/hooks.json"),
            external_file: ".dsh/hooks.json",
            bridge: Bridge {
                directory: Some(".dsh/bridge"),
                assets: &[
                    Asset {
                        destination: ".dsh/bridge/package.json",
                        content: include_str!("../generation/assets/dsh/package.json"),
                    },
                    Asset {
                        destination: ".dsh/bridge/cordis.patch.yml",
                        content: include_str!("../generation/assets/dsh/cordis.patch.yml"),
                    },
                    Asset {
                        destination: ".dsh/bridge/hook_bridge.ts",
                        content: include_str!("../generation/assets/dsh/hook_bridge.ts"),
                    },
                ],
            },
        },
        skills: Some(&Skills {
            destination: ".dsh/skills",
            source: SkillsSource::CopyCodex,
        }),
        instructions: Some(".dsh/AGENTS.md"),
        command_rules: CommandRules::None,
        config: Some(Config {
            path: ".dsh/bridge",
            sync: ConfigSync::RegisterBundle,
            directories: &[],
        }),
    },
];
