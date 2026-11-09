/// How installation keeps a provider's mutable config in step with the rendered one.
///
/// A variant names the managed operation; `config_sync` owns the code that performs it.
#[derive(Clone, Copy, Debug)]
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

/// Whether the harness renders a provider's own surfaces or bridges into a third-party agent.
///
/// Native providers lead installation and protection lists, preserving the legacy order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProviderKind {
    Native,
    Bridged,
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

#[derive(Clone, Debug)]
pub(crate) struct ProviderSpec {
    pub(crate) id: ProviderId,
    pub(crate) kind: ProviderKind,
    pub(crate) key: &'static str,
    pub(crate) display_name: &'static str,
    pub(crate) home_dir: &'static str,
    pub(crate) source_hooks: &'static str,
    pub(crate) hooks_dir: &'static str,
    pub(crate) hook_manifest: &'static str,
    pub(crate) external_hook_file: &'static str,
    pub(crate) managed_asset_directory: Option<&'static str>,
    pub(crate) assets: &'static [Asset],
    pub(crate) synced_directory: Option<&'static SyncedDirectory>,
    pub(crate) instructions: Option<&'static str>,
    pub(crate) skills: Option<&'static Skills>,
    pub(crate) managed_config: Option<&'static str>,
    /// Files rendered and published individually besides the manifest and managed config.
    pub(crate) managed_files: &'static [&'static str],
    /// Files rendered inside a managed directory after that directory is synced.
    pub(crate) generated_files: &'static [&'static str],
    /// Files a managed directory takes from the source and verification requires.
    pub(crate) source_files: &'static [&'static str],
    pub(crate) permissions: bool,
    pub(crate) config_sync: Option<ConfigSync>,
}

pub(crate) const PROVIDERS: &[ProviderSpec] = &[
    ProviderSpec {
        id: ProviderId::Codex,
        kind: ProviderKind::Native,
        key: "codex",
        display_name: "Codex",
        home_dir: ".codex",
        source_hooks: "codex/hooks",
        hooks_dir: ".codex/hooks",
        hook_manifest: ".codex/hooks.json",
        external_hook_file: ".codex/hooks.json",
        managed_asset_directory: None,
        assets: &[],
        synced_directory: None,
        instructions: Some(".codex/AGENTS.md"),
        skills: Some(&Skills {
            destination: ".codex/skills",
            source: SkillsSource::RenderCodex,
        }),
        managed_config: Some(".codex/config.toml"),
        managed_files: &[".codex/rules/default.rules"],
        generated_files: &[],
        source_files: &[],
        permissions: false,
        config_sync: Some(ConfigSync::MergeToml),
    },
    ProviderSpec {
        id: ProviderId::Claude,
        kind: ProviderKind::Native,
        key: "claude",
        display_name: "Claude",
        home_dir: ".claude",
        source_hooks: "hooks",
        hooks_dir: ".claude/hooks",
        hook_manifest: ".claude/settings.json",
        external_hook_file: ".claude/settings.json",
        managed_asset_directory: None,
        assets: &[],
        synced_directory: Some(&SyncedDirectory {
            source: "claude/statusline",
            destination: ".claude/statusline",
        }),
        instructions: Some(".claude/CLAUDE.md"),
        skills: Some(&Skills {
            destination: ".claude/skills",
            source: SkillsSource::RenderClaude,
        }),
        managed_config: None,
        managed_files: &[],
        generated_files: &[
            ".claude/hooks/rules/command_permissions.json",
            ".claude/hooks/rules/protected_paths.json",
        ],
        source_files: &[
            ".claude/hooks/rules/allowed_commands.json",
            ".claude/hooks/rules/forbidden_commands.json",
            ".claude/hooks/rules/secret_commit_policy.json",
            ".claude/hooks/rules/secret_path_policy.json",
        ],
        permissions: false,
        config_sync: Some(ConfigSync::MergeClaudeSettings),
    },
    ProviderSpec {
        id: ProviderId::Devin,
        kind: ProviderKind::Bridged,
        key: "devin",
        display_name: "Devin",
        home_dir: ".devin",
        source_hooks: "devin/hooks",
        hooks_dir: ".devin/hooks",
        hook_manifest: ".config/devin/config.json",
        external_hook_file: ".devin/hooks.v1.json",
        managed_asset_directory: None,
        assets: &[],
        synced_directory: None,
        instructions: None,
        skills: None,
        managed_config: None,
        managed_files: &[],
        generated_files: &[],
        source_files: &[],
        permissions: true,
        config_sync: Some(ConfigSync::MergeTopLevelJson),
    },
    ProviderSpec {
        id: ProviderId::Hermes,
        kind: ProviderKind::Bridged,
        key: "hermes",
        display_name: "Hermes",
        home_dir: ".hermes",
        source_hooks: "hermes/hooks",
        hooks_dir: ".hermes/hooks",
        hook_manifest: ".hermes/hooks.json",
        external_hook_file: ".hermes/hooks.json",
        managed_asset_directory: Some(".hermes/plugins/agent-harness-hooks"),
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
        synced_directory: None,
        instructions: None,
        skills: None,
        managed_config: Some(".hermes/managed-config.yaml"),
        managed_files: &[],
        generated_files: &[],
        source_files: &[],
        permissions: false,
        config_sync: Some(ConfigSync::MergeYaml),
    },
    ProviderSpec {
        id: ProviderId::Pi,
        kind: ProviderKind::Bridged,
        key: "pi",
        display_name: "Pi",
        home_dir: ".pi",
        source_hooks: "pi/hooks",
        hooks_dir: ".pi/hooks",
        hook_manifest: ".pi/agent/hooks.json",
        external_hook_file: ".pi/hooks.json",
        managed_asset_directory: None,
        assets: &[Asset {
            destination: ".pi/agent/extensions/hook_bridge.ts",
            content: include_str!("../generation/assets/pi/hook_bridge.ts"),
        }],
        synced_directory: None,
        instructions: None,
        skills: None,
        managed_config: None,
        managed_files: &[],
        generated_files: &[],
        source_files: &[],
        permissions: false,
        config_sync: None,
    },
    ProviderSpec {
        id: ProviderId::Dsh,
        kind: ProviderKind::Bridged,
        key: "dsh",
        display_name: "Dsh",
        home_dir: ".dsh",
        source_hooks: "dsh/hooks",
        hooks_dir: ".dsh/hooks",
        hook_manifest: ".dsh/hooks.json",
        external_hook_file: ".dsh/hooks.json",
        managed_asset_directory: Some(".dsh/bridge"),
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
        synced_directory: None,
        instructions: Some(".dsh/AGENTS.md"),
        skills: Some(&Skills {
            destination: ".dsh/skills",
            source: SkillsSource::CopyCodex,
        }),
        managed_config: None,
        managed_files: &[],
        generated_files: &[],
        source_files: &[],
        permissions: false,
        config_sync: Some(ConfigSync::RegisterBundle),
    },
];
