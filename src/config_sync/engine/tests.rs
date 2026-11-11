use std::path::Path;

use anyhow::Result;
use serde_json::{Value, json};

use super::*;
use crate::config_sync::codec::Role;
use crate::config_sync::installed::InstalledText;
use crate::provider::ownership::ConfigFormat;

const fn sync(format: ConfigFormat, ownership: &'static [Ownership]) -> ConfigSync {
    ConfigSync {
        format,
        ownership,
        when_missing: WhenMissing::Create,
        home_command_lists: &[],
    }
}

const fn keys(scope: KeyScope, when_absent: WhenAbsent) -> Ownership {
    Ownership::Keys { scope, when_absent }
}

fn run(sync: &ConfigSync, generated: &str, installed: Option<&str>) -> Result<Synced> {
    let generated = ConfigDocument::parse(sync.format, generated, Role::Generated)?;
    let installed = parse_installed(sync, installed, None)?;
    Merge::new(sync, &generated).apply(installed)
}

fn parse_installed(
    sync: &ConfigSync,
    config: Option<&str>,
    sidecar: Option<&str>,
) -> Result<Installed> {
    Installed::parse(
        sync,
        Path::new("installed"),
        InstalledText { config, sidecar },
    )
}

fn json_of(synced: &Synced) -> Value {
    serde_json::from_str(&synced.config).unwrap()
}

const EVERY_KEY: ConfigSync = sync(
    ConfigFormat::Json,
    &[Ownership::Keys {
        scope: KeyScope::Every,
        when_absent: WhenAbsent::Keep,
    }],
);

#[test]
fn every_key_replaces_a_generated_value_as_a_whole_and_keeps_the_rest() -> Result<()> {
    let synced = run(
        &EVERY_KEY,
        r#"{"permissions": {"allow": ["new"]}, "hooks": {}}"#,
        Some(r#"{"permissions": {"allow": ["old"], "extra": 1}, "model": "m"}"#),
    )?;

    assert_eq!(
        json_of(&synced),
        json!({"permissions": {"allow": ["new"]}, "hooks": {}, "model": "m"}),
    );
    assert_eq!(synced.sidecar, None);
    Ok(())
}

#[test]
fn a_missing_installed_config_starts_empty() -> Result<()> {
    let synced = run(&EVERY_KEY, r#"{"a": 1}"#, None)?;

    assert_eq!(json_of(&synced), json!({"a": 1}));
    Ok(())
}

#[test]
fn every_key_except_leaves_the_excluded_keys_to_another_claim() -> Result<()> {
    const POLICY: ConfigSync = sync(
        ConfigFormat::Json,
        &[keys(KeyScope::EveryExcept(&["skip"]), WhenAbsent::Keep)],
    );

    let synced = run(&POLICY, r#"{"skip": 1, "take": 2}"#, Some(r#"{"skip": 0}"#))?;

    assert_eq!(json_of(&synced), json!({"skip": 0, "take": 2}));
    Ok(())
}

#[test]
fn listed_keys_ignore_other_generated_keys_and_keep_absent_ones() -> Result<()> {
    const POLICY: ConfigSync = sync(
        ConfigFormat::Json,
        &[keys(KeyScope::Listed(&["a", "b"]), WhenAbsent::Keep)],
    );

    let synced = run(
        &POLICY,
        r#"{"a": "new", "other": 1}"#,
        Some(r#"{"a": "old", "b": "kept", "other": 0}"#),
    )?;

    assert_eq!(
        json_of(&synced),
        json!({"a": "new", "b": "kept", "other": 0})
    );
    Ok(())
}

#[test]
fn listed_keys_that_leave_the_source_are_removed_when_declared_so() -> Result<()> {
    const POLICY: ConfigSync = sync(
        ConfigFormat::Json,
        &[keys(KeyScope::Listed(&["a", "b"]), WhenAbsent::Remove)],
    );

    let synced = run(&POLICY, r#"{"a": 1}"#, Some(r#"{"b": 2, "user": 3}"#))?;

    assert_eq!(json_of(&synced), json!({"a": 1, "user": 3}));
    Ok(())
}

const EXEC_ENTRIES: ConfigSync = sync(
    ConfigFormat::Json,
    &[
        Ownership::Entries {
            path: &["permissions"],
            owned: EntryMatch::StringPrefix("Exec("),
        },
        keys(KeyScope::EveryExcept(&["permissions"]), WhenAbsent::Keep),
    ],
);

#[test]
fn entries_replace_only_the_matching_items_of_each_list() -> Result<()> {
    let synced = run(
        &EXEC_ENTRIES,
        r#"{"permissions": {"allow": ["Exec(new)"], "ask": ["Exec(ask)"]}, "hooks": 1}"#,
        Some(
            r#"{"permissions": {"allow": ["Exec(old)", "Read(**)"], "deny": ["Exec(x)"]}, "v": 1}"#,
        ),
    )?;

    assert_eq!(
        json_of(&synced),
        json!({
            "permissions": {
                "allow": ["Read(**)", "Exec(new)"],
                "ask": ["Exec(ask)"],
                "deny": ["Exec(x)"]
            },
            "hooks": 1,
            "v": 1
        }),
    );
    Ok(())
}

#[test]
fn entries_leave_the_installed_lists_alone_when_nothing_is_generated() -> Result<()> {
    let synced = run(
        &EXEC_ENTRIES,
        r#"{"hooks": 1}"#,
        Some(r#"{"permissions": {"allow": ["Exec(old)"]}}"#),
    )?;

    assert_eq!(
        json_of(&synced),
        json!({"permissions": {"allow": ["Exec(old)"]}, "hooks": 1}),
    );
    Ok(())
}

#[test]
fn entries_reject_shapes_that_are_not_lists_or_mappings() {
    let installed = r#"{"permissions": {"allow": "text"}}"#;
    let generated = r#"{"permissions": {"allow": ["Exec(a)"]}}"#;
    assert!(run(&EXEC_ENTRIES, generated, Some(installed)).is_err());
    assert!(run(&EXEC_ENTRIES, r#"{"permissions": {"allow": 1}}"#, None).is_err());
    assert!(run(&EXEC_ENTRIES, r#"{"permissions": []}"#, None).is_err());
    assert!(run(&EXEC_ENTRIES, generated, Some(r#"{"permissions": "x"}"#)).is_err());
}

#[test]
fn a_list_union_adds_what_is_missing_without_duplicating() -> Result<()> {
    const UNION: ConfigSync = sync(
        ConfigFormat::Yaml,
        &[Ownership::ListUnion {
            path: &["plugins", "enabled"],
        }],
    );

    let synced = run(
        &UNION,
        "plugins:\n  enabled:\n    - ours\n",
        Some("user: 1\nplugins:\n  enabled:\n    - theirs\n    - ours\n"),
    )?;

    let parsed: serde_norway::Value = serde_norway::from_str(&synced.config)?;
    assert_eq!(
        parsed["plugins"]["enabled"],
        serde_norway::from_str::<serde_norway::Value>("- theirs\n- ours\n")?,
    );
    assert_eq!(parsed["user"].as_i64(), Some(1));
    Ok(())
}

#[test]
fn mapping_entries_replace_one_by_one_and_keep_the_others() -> Result<()> {
    const ENTRIES: ConfigSync = sync(
        ConfigFormat::Json,
        &[Ownership::MappingEntries {
            path: &["dependencies"],
        }],
    );

    let synced = run(
        &ENTRIES,
        r#"{"dependencies": {"ours": "link:/new"}}"#,
        Some(r#"{"dependencies": {"ours": "link:/old", "theirs": "1"}}"#),
    )?;

    assert_eq!(
        json_of(&synced),
        json!({"dependencies": {"ours": "link:/new", "theirs": "1"}}),
    );
    Ok(())
}

const TRACKED: ConfigSync = sync(
    ConfigFormat::Toml,
    &[Ownership::TrackedKeys {
        path: &["env", "set"],
        sidecar: ".managed.json",
        formerly_managed: &["LEGACY"],
    }],
);

fn run_tracked(generated: &str, installed: &str, sidecar: Option<&str>) -> Result<Synced> {
    let generated = ConfigDocument::parse(ConfigFormat::Toml, generated, Role::Generated)?;
    let installed = parse_installed(&TRACKED, Some(installed), sidecar)?;
    Merge::new(&TRACKED, &generated).apply(installed)
}

#[test]
fn tracked_keys_write_the_generated_ones_and_record_them() -> Result<()> {
    let synced = run_tracked("[env.set]\nA = \"1\"\n", "[env.set]\nUSER = \"u\"\n", None)?;

    assert!(synced.config.contains("A = \"1\""));
    assert!(synced.config.contains("USER = \"u\""));
    assert_eq!(
        synced.sidecar.as_deref(),
        Some("{\n  \"env_set\": [\n    \"A\"\n  ]\n}\n"),
    );
    Ok(())
}

#[test]
fn tracked_keys_remove_what_the_previous_sync_wrote_and_the_source_dropped() -> Result<()> {
    let synced = run_tracked(
        "model = \"m\"\n",
        "[env.set]\nOLD = \"1\"\nUSER = \"u\"\n",
        Some(r#"{"env_set": ["OLD"]}"#),
    )?;

    assert!(!synced.config.contains("OLD"));
    assert!(synced.config.contains("USER = \"u\""));
    assert_eq!(synced.sidecar.as_deref(), Some("{\n  \"env_set\": []\n}\n"));
    Ok(())
}

#[test]
fn tracked_keys_fall_back_to_the_formerly_managed_ones_without_a_readable_sidecar() -> Result<()> {
    let installed = "[env.set]\nLEGACY = \"1\"\nUSER = \"u\"\n";
    for sidecar in [None, Some("not json")] {
        let synced = run_tracked("model = \"m\"\n", installed, sidecar)?;
        assert!(!synced.config.contains("LEGACY"));
        assert!(synced.config.contains("USER"));
    }
    let synced = run_tracked("model = \"m\"\n", installed, Some("{}"))?;
    assert!(synced.config.contains("LEGACY"));
    Ok(())
}

#[test]
fn tracked_keys_reject_an_installed_value_that_is_not_a_table() {
    let result = run_tracked("[env.set]\nA = \"1\"\n", "env = \"x\"\n", None);

    assert!(result.is_err());
}

#[test]
fn home_command_lists_expand_a_leading_home_prefix_only_when_asked() -> Result<()> {
    const HOME: ConfigSync = ConfigSync {
        format: ConfigFormat::Json,
        ownership: &[keys(KeyScope::Every, WhenAbsent::Keep)],
        when_missing: WhenMissing::Create,
        home_command_lists: &[&["sandbox", "commands"]],
    };
    let generated = ConfigDocument::parse(
        ConfigFormat::Json,
        r#"{"sandbox": {"commands": ["~/bin/tool *", "git *", 1]}}"#,
        Role::Generated,
    )?;
    let merge = Merge::new(&HOME, &generated);

    let kept = merge.apply(parse_installed(&HOME, None, None)?)?;
    let expanded = merge
        .expanding_home(Path::new("/h"))
        .apply(parse_installed(&HOME, None, None)?)?;

    assert_eq!(
        json_of(&kept)["sandbox"]["commands"],
        json!(["~/bin/tool *", "git *", 1]),
    );
    assert_eq!(
        json_of(&expanded)["sandbox"]["commands"],
        json!(["/h/bin/tool *", "git *", 1]),
    );
    Ok(())
}

#[test]
fn a_config_that_does_not_exist_is_skipped_only_when_declared() -> Result<()> {
    const SKIPPING: ConfigSync = ConfigSync {
        when_missing: WhenMissing::Skip,
        ..EVERY_KEY
    };

    let generated = ConfigDocument::blank(ConfigFormat::Json);
    let skipping = Merge::new(&SKIPPING, &generated);
    let creating = Merge::new(&EVERY_KEY, &generated);

    assert!(skipping.skips(&parse_installed(&SKIPPING, None, None)?));
    assert!(!skipping.skips(&parse_installed(&SKIPPING, Some("{}"), None)?));
    assert!(!creating.skips(&parse_installed(&EVERY_KEY, None, None)?));
    Ok(())
}

#[test]
fn an_installed_config_that_does_not_parse_is_reported_with_its_path() {
    let text = InstalledText {
        config: Some("[]"),
        sidecar: None,
    };
    let error = Installed::parse(&EVERY_KEY, Path::new("/home/u/settings.json"), text).unwrap_err();

    assert!(format!("{error:#}").contains("/home/u/settings.json"));
}

#[test]
fn configs_of_different_formats_cannot_be_merged() -> Result<()> {
    let generated = ConfigDocument::parse(ConfigFormat::Toml, "a = 1\n", Role::Generated)?;
    let installed = parse_installed(&EVERY_KEY, None, None)?;

    assert!(Merge::new(&EVERY_KEY, &generated).apply(installed).is_err());
    Ok(())
}
