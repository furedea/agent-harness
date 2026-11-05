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
    pub(crate) name: &'static str,
    pub(crate) content: &'static str,
}

#[derive(Debug)]
pub(crate) struct ProviderSpec {
    pub(crate) id: ProviderId,
    pub(crate) key: &'static str,
    pub(crate) display_name: &'static str,
    pub(crate) home_dir: &'static str,
    pub(crate) source_hooks: &'static str,
    pub(crate) hooks_dir: &'static str,
    pub(crate) hook_manifest: &'static str,
    pub(crate) external_hook_file: &'static str,
    pub(crate) bridge_path: Option<&'static str>,
    pub(crate) assets: &'static [Asset],
    pub(crate) permissions: bool,
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
        bridge_path: None,
        assets: &[],
        permissions: true,
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
        bridge_path: Some(".hermes/plugins/agent-harness-hooks"),
        assets: &[
            Asset {
                name: "plugin.yaml",
                content: include_str!("../generation/assets/hermes/plugin.yaml"),
            },
            Asset {
                name: "__init__.py",
                content: include_str!("../generation/assets/hermes/__init__.py"),
            },
        ],
        permissions: false,
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
        bridge_path: Some(".pi/agent/extensions/hook_bridge.ts"),
        assets: &[Asset {
            name: "hook_bridge.ts",
            content: include_str!("../generation/assets/pi/hook_bridge.ts"),
        }],
        permissions: false,
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
        bridge_path: Some(".dsh/bridge"),
        assets: &[
            Asset {
                name: "package.json",
                content: include_str!("../generation/assets/dsh/package.json"),
            },
            Asset {
                name: "cordis.patch.yml",
                content: include_str!("../generation/assets/dsh/cordis.patch.yml"),
            },
            Asset {
                name: "hook_bridge.ts",
                content: include_str!("../generation/assets/dsh/hook_bridge.ts"),
            },
        ],
        permissions: false,
    },
];
