//! `nix/providers.json` is generated from the Rust registry, which is the single source.
//!
//! The test renders the JSON from `PROVIDERS` and fails when the committed file differs.
//! Regenerate it with `UPDATE_PROVIDERS_JSON=1 cargo test --test nix_provider_registry`.

use std::collections::BTreeSet;
use std::path::PathBuf;

use serde_json::{Map, Value, json};

#[allow(dead_code)]
#[path = "../src/provider/id.rs"]
mod id;

#[allow(dead_code)]
#[path = "../src/provider/ownership.rs"]
mod ownership;

#[allow(dead_code)]
#[path = "../src/provider/registry.rs"]
mod registry;

#[allow(dead_code)]
#[path = "../src/provider/spec.rs"]
mod spec;

use spec::{ConfigFormat, ConfigLocation, ConfigSpec, ProviderSpec, RenderedConfig, SkillSource};

const UPDATE_VARIABLE: &str = "UPDATE_PROVIDERS_JSON";

#[test]
fn nix_providers_json_is_generated_from_the_registry() {
    let generated = generated_json();
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("nix/providers.json");
    if std::env::var_os(UPDATE_VARIABLE).is_some() {
        std::fs::write(&path, &generated).unwrap();
        return;
    }
    let committed = std::fs::read_to_string(&path).unwrap();
    assert!(
        committed == generated,
        "nix/providers.json is stale; regenerate it with \
         `{UPDATE_VARIABLE}=1 cargo test --test nix_provider_registry`"
    );
}

#[test]
fn every_provider_key_is_unique() {
    let keys: BTreeSet<_> = spec::PROVIDERS.iter().map(|p| p.key).collect();
    assert_eq!(keys.len(), spec::PROVIDERS.len(), "duplicate provider keys");
}

fn generated_json() -> String {
    let providers: Vec<Value> = spec::PROVIDERS.iter().map(provider_json).collect();
    let mut text = serde_json::to_string_pretty(&providers).unwrap();
    text.push('\n');
    text
}

fn provider_json(spec: &ProviderSpec) -> Value {
    json!({
        "id": spec.key,
        "display_name": spec.display_name,
        "option_description": format!("Whether to install {} harness files.", spec.display_name),
        "home_dir": spec.home_dir,
        "hook_manifest": spec.hook.registration.path(),
        "linked_paths": linked_paths(spec),
        "sync": spec.config.map_or(Value::Null, |config| sync_json(spec, config)),
        "extra_options": extra_options(spec),
    })
}

/// Paths Home Manager links: everything rendered except mutable files that sync into user config.
fn linked_paths(spec: &ProviderSpec) -> Map<String, Value> {
    let links = linked_destinations(spec);
    assert!(
        !links.contains(spec.home_dir),
        "must not link the user-owned home directory of {}",
        spec.key
    );
    links
        .into_iter()
        .map(|path| (path.to_owned(), link_json(spec, path)))
        .collect()
}

fn linked_destinations(spec: &ProviderSpec) -> BTreeSet<&'static str> {
    let registration = spec.hook.registration.path();
    let mut links = BTreeSet::from([spec.hook.scripts_dir]);
    let registration_is_config = spec
        .config
        .is_some_and(|config| config.location == ConfigLocation::SameAsHookRegistration);
    // When the registration is itself the mutable config, activation syncs it instead.
    if !registration_is_config && registration.starts_with(spec.home_dir) {
        links.insert(registration);
    }
    links.extend(bridge_destinations(spec));
    links.extend(spec.instructions);
    links.extend(spec.skill.map(|skill| skill.destination));
    links.extend(spec.config_directories().iter().map(|d| d.destination));
    links.extend(spec.published_rule_file());
    links
}

fn bridge_destinations(spec: &ProviderSpec) -> BTreeSet<&'static str> {
    let bridge = spec.hook.bridge;
    for asset in bridge.assets {
        if let Some(directory) = bridge.directory {
            assert!(asset.destination.starts_with(&format!("{directory}/")));
        }
    }
    bridge
        .assets
        .iter()
        .map(|asset| bridge.directory.unwrap_or(asset.destination))
        .collect()
}

fn link_json(spec: &ProviderSpec, path: &str) -> Value {
    match spec.skill.filter(|skill| skill.destination == path) {
        Some(skill) => json!({ "skills_provider": skills_provider(skill.source) }),
        None => json!({}),
    }
}

fn skills_provider(source: SkillSource) -> &'static str {
    match source {
        SkillSource::RenderFrontmatter => "claude",
        SkillSource::RenderWithInvocationPolicy => "codex",
        SkillSource::CopyRendered(from) => from
            .spec()
            .skill
            .map_or("codex", |skill| skills_provider(skill.source)),
    }
}

fn sync_json(spec: &ProviderSpec, config: ConfigSpec) -> Value {
    // Claude's config is the settings file its hooks live in.
    let noun = if config.sync.replaces_every_key() {
        "Settings"
    } else {
        "Config"
    };
    let mut sync = json!({
        "activation": format!("agentHarness{}{noun}", capitalized(spec.key)),
        "command": if matches!(config.rendered, RenderedConfig::SourceFile(_)) {
            "sync-config --provider pi".to_owned()
        } else {
            format!("sync-{}-{}", spec.key, noun.to_lowercase())
        },
        "source": source_path(spec, config),
        "target": sync_target(spec, config),
    });
    if config.rendered == RenderedConfig::SourceTomlWithProtection {
        // Codex's merge runs at the write boundary; the others run after link generation.
        sync["after"] = json!("writeBoundary");
    }
    sync
}

fn capitalized(key: &str) -> String {
    let mut chars = key.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

fn source_path(spec: &ProviderSpec, config: ConfigSpec) -> String {
    let path = match config.location {
        ConfigLocation::SameAsHookRegistration => spec.hook.registration.path(),
        ConfigLocation::File(path) | ConfigLocation::BundleDirectory(path) => path,
    };
    path.to_owned()
}

fn sync_target(spec: &ProviderSpec, config: ConfigSpec) -> String {
    match (config.location, config.sync.format) {
        (_, ConfigFormat::Yaml) => format!("{}/config.yaml", spec.home_dir),
        (ConfigLocation::BundleDirectory(_), _) => {
            format!("{}/profiles/${{profile}}/package.json", spec.home_dir)
        }
        _ => source_path(spec, config),
    }
}

fn extra_options(spec: &ProviderSpec) -> Value {
    match spec.config.map(|config| config.location) {
        Some(ConfigLocation::BundleDirectory(_)) => json!({
            "profile": {
                "default": "web",
                "description": "User-owned dsh profile to register the hook bundle in.",
            }
        }),
        _ => json!({}),
    }
}
