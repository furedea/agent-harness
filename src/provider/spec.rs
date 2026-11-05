#[derive(Clone, Copy, Debug)]
pub(crate) enum ConfigSync {
    MergeTopLevelJson,
    MergeYaml,
    RegisterBundle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProviderId {
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

#[derive(Clone, Debug)]
pub(crate) struct ProviderSpec {
    pub(crate) id: ProviderId,
    pub(crate) key: &'static str,
    pub(crate) display_name: &'static str,
    pub(crate) home_dir: &'static str,
    pub(crate) source_hooks: &'static str,
    pub(crate) hooks_dir: &'static str,
    pub(crate) hook_manifest: &'static str,
    pub(crate) external_hook_file: &'static str,
    pub(crate) managed_asset_directory: Option<&'static str>,
    pub(crate) assets: &'static [Asset],
    pub(crate) instructions: Option<&'static str>,
    pub(crate) skills: Option<&'static str>,
    pub(crate) managed_config: Option<&'static str>,
    pub(crate) permissions: bool,
    pub(crate) config_sync: Option<ConfigSync>,
}

pub(crate) const PROVIDERS: &[ProviderSpec] = &[
    ProviderSpec {
        id: ProviderId::Devin,
        key: "devin",
        display_name: "Devin",
        home_dir: ".devin",
        source_hooks: "devin/hooks",
        hooks_dir: ".devin/hooks",
        hook_manifest: ".config/devin/config.json",
        external_hook_file: ".devin/hooks.v1.json",
        managed_asset_directory: None,
        assets: &[],
        instructions: None,
        skills: None,
        managed_config: None,
        permissions: true,
        config_sync: Some(ConfigSync::MergeTopLevelJson),
    },
    ProviderSpec {
        id: ProviderId::Hermes,
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
        instructions: None,
        skills: None,
        managed_config: Some(".hermes/managed-config.yaml"),
        permissions: false,
        config_sync: Some(ConfigSync::MergeYaml),
    },
    ProviderSpec {
        id: ProviderId::Pi,
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
        instructions: None,
        skills: None,
        managed_config: None,
        permissions: false,
        config_sync: None,
    },
    ProviderSpec {
        id: ProviderId::Dsh,
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
        instructions: Some(".dsh/AGENTS.md"),
        skills: Some(".dsh/skills"),
        managed_config: None,
        permissions: false,
        config_sync: Some(ConfigSync::RegisterBundle),
    },
];
