use super::spec::{
    Asset, Bridge, BundleCapture, CommandRuleSpec, ConfigFormat, ConfigLocation, ConfigSpec,
    EntryMatch, HookFormat, HookRegistration, HookSection, HookSpec, KeyScope, Ownership,
    PROVIDERS, ProviderId, ProviderSpec, RenderedConfig, SkillSource, SkillSpec, SyncTarget,
    WhenAbsent,
};
use crate::{
    config_sync::installed::InstalledConfigs,
    generation::{GenerationContext, protection},
    install_plan,
    inventory::shorten_hook_path_for,
    layout::{InstalledLayout, SourceLayout},
    provider::renderer::ProviderRenderer,
    render::{self, RenderRequest},
    runtime_root::RuntimeRoot,
    shell::{
        LoadInputs,
        fixtures::{load_source, load_source_with},
    },
};
use anyhow::Result;
use clap::ValueEnum;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[test]
fn extra_spec_flows_through_layout_protection_rendering_and_publication() -> Result<()> {
    let extra = ProviderSpec {
        key: "example",
        display_name: "Example",
        home_dir: ".example",
        hook: HookSpec {
            source: "hooks",
            scripts_dir: ".example/hooks",
            registration: HookRegistration::File(".example/hooks.json"),
            format: HookFormat::BareEvents,
            section: HookSection::Optional,
            external_file: ".example/external.json",
            bundle: BundleCapture::NOTHING,
            bridge: Bridge {
                directory: Some(".example/bridge"),
                assets: &[Asset {
                    destination: ".example/bridge/nested/entry.txt",
                    content: "example bridge\n",
                }],
            },
        },
        instructions: Some(".example/AGENTS.md"),
        skill: Some(&SkillSpec {
            destination: ".example/skills",
            source: SkillSource::CopyRendered(ProviderId::Codex),
        }),
        config: Some(ConfigSpec {
            location: ConfigLocation::File(".example/managed.yaml"),
            rendered: RenderedConfig::Fixed("plugins:\n  enabled:\n    - agent-harness-hooks\n"),
            sync: ProviderId::Hermes.spec().config.unwrap().sync,
            sync_target: SyncTarget::SameAsRendered,
            directories: &[],
        }),
        ..ProviderId::Hermes.spec().clone()
    };
    let mut providers = PROVIDERS.to_vec();
    providers.push(extra);
    let staging = crate::shell::TemporaryDirectory::create()?;
    let destination = crate::shell::TemporaryDirectory::create()?;
    let layout = InstalledLayout::new(staging.path());
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/complete-source");
    let loaded = load_source_with(&source, &LoadInputs::new(&providers))?;
    let context = GenerationContext::home(&loaded);
    assert!(
        layout
            .managed_directories_for(&providers)
            .contains(&staging.path().join(".example/hooks"))
    );
    assert!(
        layout
            .managed_directories_for(&providers)
            .contains(&staging.path().join(".example/bridge"))
    );
    assert!(
        layout
            .managed_files_for(&providers)
            .contains(&staging.path().join(".example/hooks.json"))
    );
    assert!(
        layout
            .synced_directories_for(SourceLayout::new(&source), &providers)
            .contains(&(source.join("hooks"), staging.path().join(".example/hooks")))
    );
    let protected = protection::protected_paths_for_providers(context, &providers)?;
    for path in [
        "~/.example/hooks/guard.sh",
        "~/.example/bridge/nested/entry.txt",
        "~/.example/hooks.json",
        "~/.example/AGENTS.md",
        "~/.example/managed.yaml",
    ] {
        assert!(
            protected.iter().any(|entry| entry == path),
            "missing protection: {path}"
        );
    }
    assert_eq!(
        shorten_hook_path_for("$HOME/.example/hooks/guard.sh", &providers),
        "guard.sh"
    );
    let runtime_root = RuntimeRoot::home();
    let request = RenderRequest {
        dsh_profile: "web",
        source: &source,
        external_hooks: &[],
        runtime_root: &runtime_root,
    };
    render::render_installation_for(&request, staging.path(), &providers)?;
    let policy = std::fs::read_to_string(
        staging
            .path()
            .join(".claude/hooks/rules/protected_paths.json"),
    )?;
    assert!(policy.contains("~/.example/bridge/nested/entry.txt"));
    let plan = install_plan::plan_install_for(
        &layout,
        &InstalledLayout::new(destination.path()),
        &providers,
    );
    crate::shell::execute(&plan)?;
    for root in [staging.path(), destination.path()] {
        for path in [
            ".example/hooks/guard.sh",
            ".example/hooks.json",
            ".example/bridge/nested/entry.txt",
            ".example/AGENTS.md",
            ".example/skills/example-skill/SKILL.md",
            ".example/managed.yaml",
        ] {
            assert!(root.join(path).is_file(), "missing file: {path}");
        }
        assert_eq!(
            std::fs::read(root.join(".example/bridge/nested/entry.txt"))?,
            b"example bridge\n"
        );
    }
    Ok(())
}

#[test]
fn registry_derivation_preserves_install_order() {
    let layout = InstalledLayout::relative();
    assert_eq!(
        layout.managed_directories_for(PROVIDERS),
        [
            ".codex/hooks",
            ".codex/skills",
            ".claude/hooks",
            ".claude/statusline",
            ".claude/skills",
            ".devin/hooks",
            ".hermes/hooks",
            ".hermes/plugins/agent-harness-hooks",
            ".pi/hooks",
            ".pi/agent/skills",
            ".dsh/hooks",
            ".dsh/bridge",
            ".dsh/skills",
        ]
        .map(PathBuf::from)
    );
    assert_eq!(
        layout.managed_files_for(PROVIDERS),
        [
            ".codex/AGENTS.md",
            ".codex/hooks.json",
            ".codex/config.toml",
            ".codex/rules/default.rules",
            ".claude/CLAUDE.md",
            ".claude/settings.json",
            ".config/devin/config.json",
            ".hermes/hooks.json",
            ".hermes/managed-config.yaml",
            ".pi/agent/AGENTS.md",
            ".pi/agent/hooks.json",
            ".pi/agent/settings.json",
            ".pi/agent/extensions/hook_bridge.ts",
            ".dsh/AGENTS.md",
            ".dsh/hooks.json",
        ]
        .map(PathBuf::from)
    );
}

#[test]
fn registry_declares_every_provider_id_once() {
    let declared: Vec<_> = PROVIDERS.iter().map(|spec| spec.id).collect();
    let declared_set: BTreeSet<_> = declared.iter().copied().collect();
    let variants: BTreeSet<_> = ProviderId::value_variants().iter().copied().collect();
    assert_eq!(
        declared.len(),
        declared_set.len(),
        "a provider is declared twice"
    );
    assert_eq!(declared_set, variants);
}

#[test]
fn a_provider_renders_after_the_provider_whose_skills_it_copies() {
    let ids: Vec<_> = PROVIDERS.iter().map(|spec| spec.id).collect();
    let copies = PROVIDERS
        .iter()
        .filter_map(|spec| match spec.skill?.source {
            SkillSource::CopyRendered(from) => Some((spec.id, from)),
            SkillSource::RenderFrontmatter | SkillSource::RenderWithInvocationPolicy => None,
        });
    for (copier, from) in copies {
        let from_index = ids.iter().position(|id| *id == from);
        let copier_index = ids.iter().position(|id| *id == copier);
        assert!(
            from_index < copier_index,
            "{copier:?} copies skills that {from:?} renders later"
        );
    }
}

#[test]
fn every_provider_declares_the_format_of_its_mutable_config() {
    let declared: Vec<_> = PROVIDERS
        .iter()
        .map(|spec| (spec.key, spec.config.map(|config| config.sync.format)))
        .collect();
    assert_eq!(
        declared,
        [
            ("codex", Some(ConfigFormat::Toml)),
            ("claude", Some(ConfigFormat::Json)),
            ("devin", Some(ConfigFormat::Json)),
            ("hermes", Some(ConfigFormat::Yaml)),
            ("pi", Some(ConfigFormat::Json)),
            ("dsh", Some(ConfigFormat::Json)),
        ]
    );
}

#[test]
fn claude_replaces_every_generated_key_and_devin_owns_only_exec_entries() {
    let owned = |id: ProviderId| id.spec().config.unwrap().sync.ownership;

    assert_eq!(
        owned(ProviderId::Claude),
        [Ownership::Keys {
            scope: KeyScope::Every,
            when_absent: WhenAbsent::Keep
        }]
    );
    assert_eq!(
        owned(ProviderId::Devin)[0],
        Ownership::Entries {
            path: &["permissions"],
            owned: EntryMatch::StringPrefix("Exec(")
        }
    );
}

#[test]
fn a_claim_never_overlaps_another_claim_of_the_same_config() {
    for spec in PROVIDERS {
        let claims = spec.config.unwrap().sync.ownership;
        let paths: Vec<_> = claims.iter().filter_map(claim_path).collect();
        for (index, path) in paths.iter().enumerate() {
            let overlaps = paths[index + 1..]
                .iter()
                .any(|other| path.starts_with(other) || other.starts_with(path));
            assert!(!overlaps, "{} claims {path:?} twice", spec.key);
        }
    }
}

fn claim_path(claim: &Ownership) -> Option<&'static [&'static str]> {
    match claim {
        Ownership::Entries { path, .. }
        | Ownership::ListUnion { path }
        | Ownership::MappingEntries { path }
        | Ownership::TrackedKeys { path, .. } => Some(path),
        Ownership::Keys { .. } => None,
    }
}

#[test]
fn claude_and_devin_keep_hook_registration_and_config_in_one_file() {
    for (id, registration, path) in [
        (
            ProviderId::Claude,
            HookRegistration::InSettings(".claude/settings.json"),
            ".claude/settings.json",
        ),
        (
            ProviderId::Devin,
            HookRegistration::File(".config/devin/config.json"),
            ".config/devin/config.json",
        ),
    ] {
        let spec = id.spec();
        assert_eq!(spec.hook.registration, registration);
        assert_eq!(
            spec.config.map(|config| config.location),
            Some(ConfigLocation::SameAsHookRegistration)
        );
        assert_eq!(spec.config_path(), Some(path));
        assert_eq!(spec.config_file(), None, "{} is published once", spec.key);
    }
    assert_eq!(
        ProviderId::Codex.spec().config_file(),
        Some(".codex/config.toml")
    );
    assert_eq!(ProviderId::Dsh.spec().config_file(), None);
    assert_eq!(ProviderId::Dsh.spec().config_path(), Some(".dsh/bridge"));
}

#[test]
fn every_provider_declares_how_it_enforces_command_rules() {
    let claude = ProviderId::Claude.spec();
    assert!(matches!(
        claude.command_rule,
        Some(CommandRuleSpec::RuleFiles { .. })
    ));
    assert_eq!(
        claude.generated_rule_files(),
        [
            ".claude/hooks/rules/command_permissions.json",
            ".claude/hooks/rules/protected_paths.json"
        ]
    );
    assert_eq!(claude.source_rule_files().len(), 4);
    let without_rules: Vec<_> = PROVIDERS
        .iter()
        .filter(|spec| spec.command_rule.is_none())
        .map(|spec| spec.key)
        .collect();
    assert_eq!(without_rules, ["hermes", "pi", "dsh"]);
    assert_eq!(
        ProviderId::Codex.spec().command_rule,
        Some(CommandRuleSpec::ExecpolicyFile {
            rules_file: ".codex/rules/default.rules"
        })
    );
    assert_eq!(
        ProviderId::Devin.spec().command_rule,
        Some(CommandRuleSpec::InHookRegistration)
    );
}

#[test]
fn claude_and_codex_declare_their_instructions_skills_and_statusline() {
    let claude = ProviderId::Claude.spec();
    let codex = ProviderId::Codex.spec();
    assert_eq!(claude.instructions, Some(".claude/CLAUDE.md"));
    assert_eq!(codex.instructions, Some(".codex/AGENTS.md"));
    assert_eq!(
        claude.skill.map(|skill| (skill.destination, skill.source)),
        Some((".claude/skills", SkillSource::RenderFrontmatter))
    );
    assert_eq!(
        codex.skill.map(|skill| (skill.destination, skill.source)),
        Some((".codex/skills", SkillSource::RenderWithInvocationPolicy))
    );
    assert_eq!(
        claude
            .config
            .iter()
            .flat_map(|config| config.directories)
            .map(|directory| (directory.source, directory.destination))
            .collect::<Vec<_>>(),
        [("claude/statusline", ".claude/statusline")]
    );
}

#[test]
fn rendering_writes_command_rules_where_the_provider_declares_them() -> Result<()> {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/complete-source");
    let out = crate::shell::TemporaryDirectory::create()?;
    let loaded = load_source(&source)?;
    let installed = InstalledConfigs::default();
    let renderer = ProviderRenderer {
        context: GenerationContext::home(&loaded),
        providers: PROVIDERS,
        installed: &installed,
        out: out.path(),
    };
    let codex_elsewhere = ProviderSpec {
        command_rule: Some(CommandRuleSpec::ExecpolicyFile {
            rules_file: ".codex/rules/elsewhere.rules",
        }),
        ..ProviderId::Codex.spec().clone()
    };
    let codex_without_rules = ProviderSpec {
        command_rule: None,
        ..ProviderId::Codex.spec().clone()
    };

    crate::shell::execute(&renderer.render(&codex_elsewhere)?)?;
    assert!(out.path().join(".codex/rules/elsewhere.rules").is_file());
    assert!(!out.path().join(".codex/rules/default.rules").exists());
    let rendered = crate::shell::TemporaryDirectory::create()?;
    let without_rules = ProviderRenderer {
        out: rendered.path(),
        ..renderer
    }
    .render(&codex_without_rules)?;
    assert!(without_rules.iter().all(|operation| {
        !operation
            .target()
            .starts_with(rendered.path().join(".codex/rules"))
    }));
    Ok(())
}

#[test]
fn installed_layout_derives_sync_targets_from_the_registry() {
    let layout = crate::layout::InstalledLayout::relative();
    let files = layout.generated_files_for("web", PROVIDERS);
    let tail: Vec<_> = files.iter().rev().take(2).rev().collect();
    assert_eq!(
        tail,
        [
            Path::new(".hermes/config.yaml"),
            Path::new(".dsh/profiles/web/package.json"),
        ]
    );
    assert_eq!(
        layout
            .generated_files_for("other", PROVIDERS)
            .last()
            .map(|p| p.to_owned()),
        Some(PathBuf::from(".dsh/profiles/other/package.json"))
    );
}

#[test]
fn hook_bundle_capture_is_declared_by_codex_alone() {
    let capturing: Vec<_> = PROVIDERS
        .iter()
        .filter(|spec| {
            spec.hook.bundle.config_file.is_some()
                || spec.hook.bundle.top_level_scripts_dir.is_some()
        })
        .map(|spec| spec.key)
        .collect();
    assert_eq!(capturing, ["codex"]);
    let codex = ProviderId::Codex.spec().hook.bundle;
    assert_eq!(codex.config_file, Some(".codex/config.toml"));
    assert_eq!(codex.top_level_scripts_dir, Some(".codex"));
    assert_eq!(
        crate::generation::hook_bundle::supported_files().last(),
        Some(".codex/config.toml")
    );
}
