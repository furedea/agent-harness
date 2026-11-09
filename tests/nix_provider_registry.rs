use std::collections::BTreeSet;

#[allow(dead_code)]
#[path = "../src/provider/spec.rs"]
mod spec;

#[test]
fn nix_providers_match_rust_registry_and_managed_link_destinations() {
    let providers: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("../nix/providers.json")).unwrap();
    let ids: BTreeSet<_> = providers
        .iter()
        .map(|p| p["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids.len(), providers.len(), "duplicate Nix provider ids");
    assert_eq!(ids, spec::PROVIDERS.iter().map(|p| p.key).collect());
    for provider in &providers {
        let rust = spec::PROVIDERS
            .iter()
            .find(|p| p.key == provider["id"])
            .unwrap();
        assert_eq!(provider["home_dir"], rust.home_dir);
        assert_eq!(provider["hook_manifest"], rust.hooks.registration.path());
        assert_eq!(provider["display_name"], rust.display_name);
        assert_eq!(provider["sync"].is_null(), rust.config.is_none());
        if let Some(config) = rust.config {
            assert_eq!(provider["sync"]["source"], config.path);
        }
        let links = provider["linked_paths"].as_object().unwrap();
        assert_eq!(
            links.keys().map(String::as_str).collect::<BTreeSet<_>>(),
            expected_links(provider, rust)
        );
        assert!(
            !links.contains_key(rust.home_dir),
            "must not link user-owned home directories"
        );
        assert_skills_links(links, rust);
    }
}

/// Paths Home Manager links: everything rendered except mutable files that sync into user config.
fn expected_links<'a>(
    provider: &serde_json::Value,
    rust: &'a spec::ProviderSpec,
) -> BTreeSet<&'a str> {
    let registration = rust.hooks.registration.path();
    let mut expected = BTreeSet::from([rust.hooks.scripts_dir]);
    if provider["sync"]["source"] == registration {
        // The registration is itself the mutable config, so activation syncs it instead.
    } else if registration.starts_with(rust.home_dir) {
        expected.insert(registration);
    }
    for asset in rust.hooks.bridge.assets {
        if let Some(directory) = rust.hooks.bridge.directory {
            assert!(asset.destination.starts_with(&format!("{directory}/")));
        }
        expected.insert(rust.hooks.bridge.directory.unwrap_or(asset.destination));
    }
    expected.extend(rust.instructions);
    expected.extend(rust.skills.map(|skills| skills.destination));
    expected.extend(
        rust.config
            .iter()
            .flat_map(|config| config.directories)
            .map(|directory| directory.destination),
    );
    expected.extend(rust.command_rules.published_file());
    expected
}

fn assert_skills_links(
    links: &serde_json::Map<String, serde_json::Value>,
    rust: &spec::ProviderSpec,
) {
    let Some(skills) = rust.skills else { return };
    let rendered_for = match skills.source {
        spec::SkillsSource::RenderClaude => "claude",
        spec::SkillsSource::RenderCodex | spec::SkillsSource::CopyCodex => "codex",
    };
    assert_eq!(
        links[skills.destination]["skills_provider"], rendered_for,
        "{} skills must come from the {rendered_for} skills",
        rust.key
    );
}
