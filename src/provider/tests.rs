use super::spec::{
    Asset, ConfigSync, PROVIDERS, ProviderId, ProviderKind, ProviderSpec, Skills, SkillsSource,
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
        source_hooks: "hooks",
        hooks_dir: ".example/hooks",
        hook_manifest: ".example/hooks.json",
        external_hook_file: ".example/external.json",
        managed_asset_directory: Some(".example/bridge"),
        assets: &[Asset {
            destination: ".example/bridge/nested/entry.txt",
            content: "example bridge\n",
        }],
        instructions: Some(".example/AGENTS.md"),
        skills: Some(&Skills {
            destination: ".example/skills",
            source: SkillsSource::CopyCodex,
        }),
        managed_config: Some(".example/managed.yaml"),
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
            ".claude/hooks",
            ".devin/hooks",
            ".hermes/hooks",
            ".hermes/plugins/agent-harness-hooks",
            ".pi/hooks",
            ".dsh/hooks",
            ".dsh/bridge",
            ".dsh/skills",
            ".claude/statusline",
            ".codex/skills",
            ".claude/skills",
        ]
        .map(PathBuf::from)
    );
    assert_eq!(
        layout.managed_files_for(PROVIDERS),
        [
            ".codex/AGENTS.md",
            ".claude/CLAUDE.md",
            ".codex/hooks.json",
            ".claude/settings.json",
            ".codex/rules/default.rules",
            ".codex/config.toml",
            ".config/devin/config.json",
            ".hermes/hooks.json",
            ".hermes/managed-config.yaml",
            ".pi/agent/hooks.json",
            ".dsh/hooks.json",
            ".dsh/AGENTS.md",
            ".pi/agent/extensions/hook_bridge.ts",
        ]
        .map(PathBuf::from)
    );
}

#[test]
fn registry_declares_all_six_providers_with_native_ones_first() {
    let declared: Vec<_> = PROVIDERS.iter().map(|spec| (spec.id, spec.kind)).collect();
    assert_eq!(
        declared,
        [
            (ProviderId::Codex, ProviderKind::Native),
            (ProviderId::Claude, ProviderKind::Native),
            (ProviderId::Devin, ProviderKind::Bridged),
            (ProviderId::Hermes, ProviderKind::Bridged),
            (ProviderId::Pi, ProviderKind::Bridged),
            (ProviderId::Dsh, ProviderKind::Bridged),
        ]
    );
}

#[test]
fn every_provider_declares_the_sync_kind_of_its_mutable_config() {
    let declared: Vec<_> = PROVIDERS
        .iter()
        .map(|spec| (spec.key, spec.config_sync.map(|sync| format!("{sync:?}"))))
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
    assert!(matches!(
        ProviderId::Claude.spec().config_sync,
        Some(ConfigSync::MergeClaudeSettings)
    ));
}

#[test]
fn claude_and_codex_declare_their_instructions_skills_and_statusline() {
    let claude = ProviderId::Claude.spec();
    let codex = ProviderId::Codex.spec();
    assert_eq!(claude.instructions, Some(".claude/CLAUDE.md"));
    assert_eq!(codex.instructions, Some(".codex/AGENTS.md"));
    assert_eq!(
        claude
            .skills
            .map(|skills| (skills.destination, skills.source)),
        Some((".claude/skills", SkillsSource::RenderClaude))
    );
    assert_eq!(
        codex
            .skills
            .map(|skills| (skills.destination, skills.source)),
        Some((".codex/skills", SkillsSource::RenderCodex))
    );
    assert_eq!(
        claude
            .synced_directory
            .map(|directory| (directory.source, directory.destination)),
        Some(("claude/statusline", ".claude/statusline"))
    );
}

#[test]
fn rendering_fails_when_a_native_provider_drops_a_declared_generated_file() -> Result<()> {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/complete-source");
    let out = crate::filesystem::TemporaryDirectory::create()?;
    let renderer = ProviderRenderer {
        context: GenerationContext::home(&source, &[]),
        providers: PROVIDERS,
        out: out.path(),
    };
    let codex_without_rules = ProviderSpec {
        managed_files: &[],
        ..ProviderId::Codex.spec().clone()
    };
    let claude_without_policies = ProviderSpec {
        generated_files: &[],
        ..ProviderId::Claude.spec().clone()
    };

    let codex = renderer.render(&codex_without_rules).unwrap_err();
    let claude = renderer.render(&claude_without_policies).unwrap_err();

    assert!(codex.to_string().contains("exactly one rules file"));
    assert!(claude.to_string().contains("protected paths"));
    Ok(())
}
