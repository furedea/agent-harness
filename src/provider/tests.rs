use super::spec::{
    Asset, Bridge, CommandRuleSpec, ConfigSpec, ConfigSync, HookRegistration, HookSpec, PROVIDERS,
    ProviderId, ProviderSpec, SkillSource, SkillSpec,
};
use crate::{
    filesystem::RealFileSystem,
    generation::{GenerationContext, protection},
    install_executor, install_plan,
    inventory::shorten_hook_path_for,
    layout::{InstalledLayout, SourceLayout},
    provider::renderer::ProviderRenderer,
    render,
};
use anyhow::Result;
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
            external_file: ".example/external.json",
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
            source: SkillSource::CopyCodex,
        }),
        config: Some(ConfigSpec {
            path: ".example/managed.yaml",
            sync: ConfigSync::MergeYaml,
            directories: &[],
        }),
        ..ProviderId::Hermes.spec().clone()
    };
    let mut providers = PROVIDERS.to_vec();
    providers.push(extra);
    let staging = crate::filesystem::TemporaryDirectory::create()?;
    let destination = crate::filesystem::TemporaryDirectory::create()?;
    let layout = InstalledLayout::new(staging.path());
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/complete-source");
    let context = GenerationContext::home(&source, &[]);
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
    render::render_installation_for(context, staging.path(), &providers)?;
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
    install_executor::execute(&RealFileSystem, &plan)?;
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
            ".pi/agent/hooks.json",
            ".pi/agent/extensions/hook_bridge.ts",
            ".dsh/AGENTS.md",
            ".dsh/hooks.json",
        ]
        .map(PathBuf::from)
    );
}

#[test]
fn registry_declares_all_six_providers() {
    let declared: Vec<_> = PROVIDERS.iter().map(|spec| spec.id).collect();
    assert_eq!(
        declared,
        [
            ProviderId::Codex,
            ProviderId::Claude,
            ProviderId::Devin,
            ProviderId::Hermes,
            ProviderId::Pi,
            ProviderId::Dsh,
        ]
    );
}

#[test]
fn every_provider_declares_the_sync_kind_of_its_mutable_config() {
    let declared: Vec<_> = PROVIDERS
        .iter()
        .map(|spec| {
            (
                spec.key,
                spec.config.map(|config| format!("{:?}", config.sync)),
            )
        })
        .collect();
    assert_eq!(
        declared,
        [
            ("codex", Some("MergeToml".to_owned())),
            ("claude", Some("MergeClaudeSettings".to_owned())),
            ("devin", Some("MergeTopLevelJson".to_owned())),
            ("hermes", Some("MergeYaml".to_owned())),
            ("pi", None),
            ("dsh", Some("RegisterBundle".to_owned())),
        ]
    );
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
        assert_eq!(spec.config.map(|config| config.path), Some(path));
        assert_eq!(spec.config_file(), None, "{} is published once", spec.key);
    }
    assert_eq!(
        ProviderId::Codex.spec().config_file(),
        Some(".codex/config.toml")
    );
    assert_eq!(ProviderId::Dsh.spec().config_file(), None);
}

#[test]
fn every_provider_declares_how_it_enforces_command_rules() {
    let claude = ProviderId::Claude.spec();
    assert!(matches!(
        claude.command_rule,
        Some(CommandRuleSpec::ClaudeRules { .. })
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
        Some(CommandRuleSpec::CodexExecpolicy {
            rules_file: ".codex/rules/default.rules"
        })
    );
    assert_eq!(
        ProviderId::Devin.spec().command_rule,
        Some(CommandRuleSpec::DevinHookBlock)
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
        Some((".claude/skills", SkillSource::RenderClaude))
    );
    assert_eq!(
        codex.skill.map(|skill| (skill.destination, skill.source)),
        Some((".codex/skills", SkillSource::RenderCodex))
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
    let out = crate::filesystem::TemporaryDirectory::create()?;
    let renderer = ProviderRenderer {
        context: GenerationContext::home(&source, &[]),
        providers: PROVIDERS,
        out: out.path(),
    };
    let codex_elsewhere = ProviderSpec {
        command_rule: Some(CommandRuleSpec::CodexExecpolicy {
            rules_file: ".codex/rules/elsewhere.rules",
        }),
        ..ProviderId::Codex.spec().clone()
    };
    let codex_without_rules = ProviderSpec {
        command_rule: None,
        ..ProviderId::Codex.spec().clone()
    };

    renderer.render(&codex_elsewhere)?;
    assert!(out.path().join(".codex/rules/elsewhere.rules").is_file());
    assert!(!out.path().join(".codex/rules/default.rules").exists());
    let rendered = crate::filesystem::TemporaryDirectory::create()?;
    ProviderRenderer {
        out: rendered.path(),
        ..renderer
    }
    .render(&codex_without_rules)?;
    assert!(!rendered.path().join(".codex/rules").exists());
    Ok(())
}
