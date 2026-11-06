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
        assert_eq!(provider["hook_manifest"], rust.hook_manifest);
        assert_eq!(provider["display_name"], rust.display_name);
        let mut expected = BTreeSet::from([rust.hooks_dir]);
        if rust.hook_manifest.starts_with(rust.home_dir) {
            expected.insert(rust.hook_manifest);
        } else {
            assert_eq!(provider["sync"]["source"], rust.hook_manifest);
        }
        for asset in rust.assets {
            if let Some(directory) = rust.managed_asset_directory {
                assert!(asset.destination.starts_with(&format!("{directory}/")));
            }
            expected.insert(rust.managed_asset_directory.unwrap_or(asset.destination));
        }
        if rust.key == "dsh" {
            expected.extend([".dsh/skills", ".dsh/AGENTS.md"]);
        }
        let links = provider["linked_paths"].as_object().unwrap();
        assert_eq!(
            links.keys().map(String::as_str).collect::<BTreeSet<_>>(),
            expected
        );
        assert!(
            !links.contains_key(rust.home_dir),
            "must not link user-owned home directories"
        );
    }
}
