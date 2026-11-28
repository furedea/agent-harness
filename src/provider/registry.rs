#![forbid(clippy::disallowed_methods, clippy::disallowed_types)]
//! The registry: every provider's declaration, in install order.

use super::spec::{
    Asset, Bridge, BundleCapture, CommandRuleSpec, ConfigFormat, ConfigLocation, ConfigSpec,
    ConfigSync, EntryMatch, HookFormat, HookRegistration, HookSection, HookSpec, KeyScope,
    McpEntry, McpMapping, McpSpec, Ownership, ProviderId, ProviderSpec, RenderedConfig, SIDECAR,
    SkillSource, SkillSpec, SyncedDirectory, WhenAbsent, WhenMissing,
};

impl ProviderId {
    pub(crate) fn spec(self) -> &'static ProviderSpec {
        match self {
            Self::Claude => &CLAUDE,
            Self::Codex => &CODEX,
            Self::Devin => &DEVIN,
            Self::Hermes => &HERMES,
            Self::Pi => &PI,
            Self::Dsh => &DSH,
            Self::Antigravity => &ANTIGRAVITY,
        }
    }
}

const CODEX_MANAGED_KEYS: &[&str] = &[
    "model",
    "model_reasoning_effort",
    "personality",
    "approval_policy",
    "sandbox_mode",
    "approvals_reviewer",
    "cli_auth_credentials_store",
    "notice",
    "tui",
    "plugins",
    "features",
    "default_permissions",
    "permissions",
];

const CODEX_USER_KEPT_KEYS: &[&str] = &[
    "analytics",
    "otel",
    "feedback",
    "check_for_update_on_startup",
];

const HERMES_MANAGED_CONFIG: &str = "plugins:\n  enabled:\n    - agent-harness-hooks\n";

const CODEX: ProviderSpec = ProviderSpec {
    id: ProviderId::Codex,
    key: "codex",
    display_name: "Codex",
    home_dir: ".codex",
    hook: HookSpec {
        source: "codex/hooks",
        scripts_dir: ".codex/hooks",
        registration: HookRegistration::File(".codex/hooks.json"),
        format: HookFormat::WrappedEvents,
        section: HookSection::Required,
        external_file: ".codex/hooks.json",
        bundle: BundleCapture {
            config_file: Some(".codex/config.toml"),
            top_level_scripts_dir: Some(".codex"),
        },
        bridge: Bridge {
            directory: None,
            assets: &[],
        },
    },
    skill: Some(&SkillSpec {
        destination: ".codex/skills",
        source: SkillSource::RenderWithInvocationPolicy,
    }),
    instructions: Some(".codex/AGENTS.md"),
    command_rule: Some(CommandRuleSpec::ExecpolicyFile {
        rules_file: ".codex/rules/default.rules",
    }),
    config: Some(ConfigSpec {
        location: ConfigLocation::File(".codex/config.toml"),
        rendered: RenderedConfig::SourceTomlWithGuardedFilesystem,
        sync: ConfigSync {
            format: ConfigFormat::Toml,
            ownership: &[
                Ownership::Keys {
                    scope: KeyScope::Listed(CODEX_MANAGED_KEYS),
                    when_absent: WhenAbsent::Remove,
                },
                Ownership::Keys {
                    scope: KeyScope::Listed(CODEX_USER_KEPT_KEYS),
                    when_absent: WhenAbsent::Keep,
                },
                Ownership::TrackedKeys {
                    path: &["shell_environment_policy", "set"],
                    sidecar: SIDECAR,
                    formerly_managed: &["GH_CONFIG_DIR"],
                },
            ],
            when_missing: WhenMissing::Create,
            mcp_servers: Some(McpMapping {
                path: &["mcp_servers"],
                entry: McpEntry::Url,
            }),
            home_command_lists: &[],
            home_rule_lists: &[],
        },
        directories: &[],
    }),
    // `codex mcp add` writes the servers into the config too.
    mcp: None,
    retired_files: &[],
};

const CLAUDE: ProviderSpec = ProviderSpec {
    id: ProviderId::Claude,
    key: "claude",
    display_name: "Claude",
    home_dir: ".claude",
    hook: HookSpec {
        source: "hooks",
        scripts_dir: ".claude/hooks",
        registration: HookRegistration::InSettings(".claude/settings.json"),
        format: HookFormat::SettingsSection,
        section: HookSection::Required,
        external_file: ".claude/settings.json",
        bundle: BundleCapture::NOTHING,
        bridge: Bridge {
            directory: None,
            assets: &[],
        },
    },
    skill: Some(&SkillSpec {
        destination: ".claude/skills",
        source: SkillSource::RenderFrontmatter,
    }),
    instructions: Some(".claude/CLAUDE.md"),
    command_rule: Some(CommandRuleSpec::RuleFiles {
        permissions: ".claude/hooks/rules/command_permissions.json",
        source_files: &[
            ".claude/hooks/rules/allowed_commands.json",
            ".claude/hooks/rules/forbidden_commands.json",
            ".claude/hooks/rules/secret_commit_policy.json",
            ".claude/hooks/rules/secret_path_policy.json",
        ],
    }),
    // settings.json is also the hook registration: one file, two roles.
    config: Some(ConfigSpec {
        location: ConfigLocation::SameAsHookRegistration,
        rendered: RenderedConfig::WithHooks,
        sync: ConfigSync {
            format: ConfigFormat::Json,
            ownership: &[Ownership::Keys {
                scope: KeyScope::Every,
                when_absent: WhenAbsent::Keep,
            }],
            when_missing: WhenMissing::Create,
            mcp_servers: None,
            home_command_lists: &[&["sandbox", "excludedCommands"]],
            home_rule_lists: &[],
        },
        directories: &[SyncedDirectory {
            source: "claude/statusline",
            destination: ".claude/statusline",
        }],
    }),
    mcp: Some(McpSpec::SkillsDirectoryPlugin {
        directory: "agent-harness",
    }),
    retired_files: &[],
};

const DEVIN: ProviderSpec = ProviderSpec {
    id: ProviderId::Devin,
    key: "devin",
    display_name: "Devin",
    home_dir: ".devin",
    hook: HookSpec {
        source: "devin/hooks",
        scripts_dir: ".devin/hooks",
        registration: HookRegistration::File(".config/devin/config.json"),
        format: HookFormat::BareEvents,
        section: HookSection::Optional,
        external_file: ".devin/hooks.v1.json",
        bundle: BundleCapture::NOTHING,
        bridge: Bridge {
            directory: None,
            assets: &[],
        },
    },
    skill: None,
    instructions: None,
    command_rule: Some(CommandRuleSpec::InHookRegistration),
    // config.json is also the hook registration: one file, two roles.
    config: Some(ConfigSpec {
        location: ConfigLocation::SameAsHookRegistration,
        rendered: RenderedConfig::WithHooks,
        sync: ConfigSync {
            format: ConfigFormat::Json,
            // Exec(...) entries are the harness's, as Bash(...) entries are in Claude settings;
            // the other entries of each permission list stay the user's.
            ownership: &[
                Ownership::Entries {
                    path: &["permissions"],
                    owned: EntryMatch::StringPrefix("Exec("),
                },
                Ownership::Keys {
                    scope: KeyScope::EveryExcept(&["permissions"]),
                    when_absent: WhenAbsent::Keep,
                },
            ],
            when_missing: WhenMissing::Create,
            mcp_servers: None,
            home_command_lists: &[],
            home_rule_lists: &[],
        },
        directories: &[],
    }),
    // `devin mcp add -s user` writes this file too.
    mcp: Some(McpSpec::ConfigFile(ConfigSpec::mcp_server_file(
        ".config/devin/mcp_config.json",
        McpEntry::UrlWithTransport,
    ))),
    retired_files: &[],
};

const HERMES: ProviderSpec = ProviderSpec {
    id: ProviderId::Hermes,
    key: "hermes",
    display_name: "Hermes",
    home_dir: ".hermes",
    hook: HookSpec {
        source: "hermes/hooks",
        scripts_dir: ".hermes/hooks",
        registration: HookRegistration::File(".hermes/hooks.json"),
        format: HookFormat::BareEvents,
        section: HookSection::Optional,
        external_file: ".hermes/hooks.json",
        bundle: BundleCapture::NOTHING,
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
    skill: None,
    instructions: None,
    command_rule: None,
    config: Some(ConfigSpec {
        location: ConfigLocation::File(".hermes/managed-config.yaml"),
        rendered: RenderedConfig::Fixed(HERMES_MANAGED_CONFIG),
        sync: ConfigSync {
            format: ConfigFormat::Yaml,
            ownership: &[
                Ownership::Keys {
                    scope: KeyScope::EveryExcept(&["plugins"]),
                    when_absent: WhenAbsent::Keep,
                },
                Ownership::ListUnion {
                    path: &["plugins", "enabled"],
                },
            ],
            when_missing: WhenMissing::Create,
            mcp_servers: Some(McpMapping {
                path: &["mcp_servers"],
                entry: McpEntry::Url,
            }),
            home_command_lists: &[],
            home_rule_lists: &[],
        },
        directories: &[],
    }),
    mcp: None,
    retired_files: &[],
};

const PI: ProviderSpec = ProviderSpec {
    id: ProviderId::Pi,
    key: "pi",
    display_name: "Pi",
    home_dir: ".pi",
    hook: HookSpec {
        source: "pi/hooks",
        scripts_dir: ".pi/hooks",
        registration: HookRegistration::File(".pi/agent/hooks.json"),
        format: HookFormat::BareEvents,
        section: HookSection::Optional,
        external_file: ".pi/hooks.json",
        bundle: BundleCapture::NOTHING,
        bridge: Bridge {
            directory: None,
            assets: &[Asset {
                destination: ".pi/agent/extensions/hook_bridge.ts",
                content: include_str!("../generation/assets/pi/hook_bridge.ts"),
            }],
        },
    },
    skill: Some(&SkillSpec {
        destination: ".pi/agent/skills",
        source: SkillSource::RenderFrontmatter,
    }),
    instructions: Some(".pi/agent/AGENTS.md"),
    command_rule: None,
    config: Some(ConfigSpec {
        location: ConfigLocation::File(".pi/agent/settings.json"),
        rendered: RenderedConfig::SourceFile("pi/settings.json"),
        sync: ConfigSync {
            format: ConfigFormat::Json,
            ownership: &[Ownership::Keys {
                scope: KeyScope::Listed(&[
                    "defaultProvider",
                    "defaultModel",
                    "defaultThinkingLevel",
                ]),
                when_absent: WhenAbsent::Keep,
            }],
            when_missing: WhenMissing::Create,
            mcp_servers: None,
            home_command_lists: &[],
            home_rule_lists: &[],
        },
        directories: &[],
    }),
    // Pi has no MCP client; an MCP extension is not part of what the harness installs.
    mcp: None,
    // Earlier releases installed an srt sandbox extension here (ADR-0032).
    retired_files: &[
        ".pi/agent/extensions/sandbox/file_operations.mjs",
        ".pi/agent/extensions/bash_sandbox.ts",
        ".pi/agent/extensions/sandbox_policy.json",
    ],
};

const DSH: ProviderSpec = ProviderSpec {
    id: ProviderId::Dsh,
    key: "dsh",
    display_name: "DeepSeek",
    home_dir: ".dsh",
    hook: HookSpec {
        source: "dsh/hooks",
        scripts_dir: ".dsh/hooks",
        registration: HookRegistration::File(".dsh/hooks.json"),
        format: HookFormat::BareEvents,
        section: HookSection::Optional,
        external_file: ".dsh/hooks.json",
        bundle: BundleCapture::NOTHING,
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
    skill: Some(&SkillSpec {
        destination: ".dsh/skills",
        source: SkillSource::CopyRendered(ProviderId::Codex),
    }),
    instructions: Some(".dsh/AGENTS.md"),
    command_rule: None,
    config: Some(ConfigSpec {
        location: ConfigLocation::BundleDirectory(".dsh/bridge"),
        rendered: RenderedConfig::WithHooks,
        sync: ConfigSync {
            format: ConfigFormat::Json,
            ownership: &[
                Ownership::MappingEntries {
                    path: &["dependencies"],
                },
                Ownership::ListUnion {
                    path: &["dsh", "profile", "bundles"],
                },
            ],
            // The profile is the user's, and dsh creates it on its first run.
            when_missing: WhenMissing::Skip,
            mcp_servers: None,
            home_command_lists: &[],
            home_rule_lists: &[],
        },
        directories: &[],
    }),
    // dsh reaches MCP servers only through a separate client plugin that the profile must
    // install, which the harness does not.
    mcp: None,
    retired_files: &[],
};

/// Rule kinds of `permissions` the harness writes. `agy` has no `edit_file(...)` kind.
const ANTIGRAVITY_MANAGED_RULES: &[&str] = &["command(", "read_file(", "write_file("];

const ANTIGRAVITY: ProviderSpec = ProviderSpec {
    id: ProviderId::Antigravity,
    key: "antigravity",
    display_name: "Antigravity",
    home_dir: ".gemini/config",
    hook: HookSpec {
        source: "antigravity/hooks",
        scripts_dir: ".gemini/config/hooks",
        registration: HookRegistration::File(".gemini/config/hooks.json"),
        format: HookFormat::NamedSpecs,
        section: HookSection::Optional,
        external_file: ".gemini/config/hooks.json",
        bundle: BundleCapture::NOTHING,
        bridge: Bridge {
            directory: None,
            assets: &[],
        },
    },
    skill: Some(&SkillSpec {
        destination: ".gemini/config/skills",
        source: SkillSource::RenderFrontmatter,
    }),
    instructions: Some(".gemini/config/AGENTS.md"),
    command_rule: None,
    config: Some(ConfigSpec {
        location: ConfigLocation::File(".gemini/antigravity-cli/settings.json"),
        rendered: RenderedConfig::SourceJsonWithToolPermissions("antigravity/settings.json"),
        sync: ConfigSync {
            format: ConfigFormat::Json,
            ownership: &[
                Ownership::Entries {
                    path: &["permissions"],
                    owned: EntryMatch::AnyStringPrefix(ANTIGRAVITY_MANAGED_RULES),
                },
                Ownership::Keys {
                    scope: KeyScope::EveryExcept(&["permissions"]),
                    when_absent: WhenAbsent::Keep,
                },
            ],
            when_missing: WhenMissing::Create,
            // `agy` reads absolute paths only, so the sync expands the home prefix.
            mcp_servers: None,
            home_command_lists: &[],
            home_rule_lists: &[
                &["permissions", "allow"],
                &["permissions", "ask"],
                &["permissions", "deny"],
            ],
        },
        directories: &[],
    }),
    // The editor, app, and `agy` share this file, which the MCP manager also writes.
    mcp: Some(McpSpec::ConfigFile(ConfigSpec::mcp_server_file(
        ".gemini/config/mcp_config.json",
        McpEntry::ServerUrl,
    ))),
    retired_files: &[],
};

pub(crate) const PROVIDERS: &[ProviderSpec] = &[CODEX, CLAUDE, DEVIN, HERMES, PI, DSH, ANTIGRAVITY];
