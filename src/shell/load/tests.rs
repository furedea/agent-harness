use super::*;
use crate::generation::external_hooks::ExternalHookBundle;
use crate::provider::spec::PROVIDERS;
use crate::shell::{TemporaryDirectory, fixtures};

fn load(root: &Path) -> Result<Source> {
    fixtures::load_source(root)
}

#[test]
fn a_complete_source_loads_into_typed_values() -> Result<()> {
    let root = TemporaryDirectory::create()?;
    fixtures::create_dir_all(root.path().join("skills/one"))?;
    fixtures::write(
        root.path().join("skills/one/SKILL.md"),
        "---\nname: one\ndescription: first\n---\n\n# One\n",
    )?;
    fixtures::write(root.path().join("skills/one/notes.txt"), "kept\n")?;

    let source = load(root.path())?;

    assert_eq!(source.root(), root.path());
    assert_eq!(source.skills.metadata()[0].title, "One");
    assert!(source.manifest.runtime_commands().is_empty());
    Ok(())
}

#[test]
fn a_directory_that_is_only_copied_is_a_location_and_a_file_list() -> Result<()> {
    let root = TemporaryDirectory::create()?;
    fixtures::create_dir_all(root.path().join("hooks/nested"))?;
    fixtures::write(root.path().join("hooks/a.sh"), "echo\n")?;
    fixtures::write(root.path().join("hooks/nested/b.sh"), [0xff, 0xfe])?;

    let source = load(root.path())?;

    let hooks = source
        .directory(&root.path().join("hooks"))
        .expect("the hooks directory is loaded");
    assert!(hooks.present);
    let relative: Vec<_> = hooks.relative_files().collect();
    assert_eq!(
        &relative[..2],
        [Path::new("a.sh"), Path::new("nested/b.sh")]
    );
    Ok(())
}

#[test]
fn an_optional_directory_the_source_omits_is_absent() -> Result<()> {
    let root = TemporaryDirectory::create()?;

    let source = load(root.path())?;

    let claude = PROVIDERS
        .iter()
        .flat_map(|spec| spec.config_directories())
        .map(|directory| root.path().join(directory.source))
        .next()
        .expect("a provider declares a config directory");
    let directory = source
        .directory(&claude)
        .expect("the directory is declared");
    assert!(!directory.present);
    assert!(directory.files.is_empty());
    Ok(())
}

#[test]
fn an_invalid_document_is_refused_at_the_boundary_with_its_path() -> Result<()> {
    let root = TemporaryDirectory::create()?;
    fixtures::write(root.path().join("command_permissions.json"), "{not json")?;

    let error = load(root.path()).unwrap_err();

    let message = format!("{error:#}");
    assert!(message.contains("command_permissions.json"), "{message}");
    Ok(())
}

#[test]
fn a_missing_required_document_is_refused_with_its_path() -> Result<()> {
    let root = TemporaryDirectory::create()?;
    fixtures::create_dir_all(root.path().join("hooks"))?;

    let error = load_source(root.path(), &LoadInputs::new(PROVIDERS)).unwrap_err();

    assert!(format!("{error:#}").contains("allowed_commands.json"));
    Ok(())
}

#[test]
fn a_missing_required_directory_is_refused() -> Result<()> {
    let root = TemporaryDirectory::create()?;
    let source = load(root.path())?;
    fixtures::remove_dir_all(source.root().join("hooks"))?;

    let error = load_source(root.path(), &LoadInputs::new(PROVIDERS)).unwrap_err();

    assert!(format!("{error:#}").contains("missing required harness source directory"));
    Ok(())
}

#[test]
fn a_skill_without_a_title_is_refused_with_its_path() -> Result<()> {
    let root = TemporaryDirectory::create()?;
    fixtures::create_dir_all(root.path().join("skills/one"))?;
    fixtures::write(
        root.path().join("skills/one/SKILL.md"),
        "---\nname: one\ndescription: first\n---\n\nbody\n",
    )?;

    let error = load(root.path()).unwrap_err();

    assert!(format!("{error:#}").contains("skills/one/SKILL.md"));
    Ok(())
}

#[test]
fn text_is_absent_for_a_missing_path_and_an_error_for_a_directory() -> Result<()> {
    let root = TemporaryDirectory::create()?;

    assert_eq!(read_text_if_present(&root.path().join("missing"))?, None);
    assert!(read_text_if_present(root.path()).is_err());
    Ok(())
}

#[test]
fn a_file_that_is_not_utf8_cannot_be_read_as_text() -> Result<()> {
    let root = TemporaryDirectory::create()?;
    let binary = root.path().join("binary");
    fixtures::write(&binary, [0xff, 0xfe])?;

    let error = read_text(&binary).unwrap_err();

    assert!(format!("{error:#}").contains("binary"));
    Ok(())
}

#[test]
fn an_installed_config_that_cannot_be_parsed_is_refused_with_its_path() -> Result<()> {
    let root = TemporaryDirectory::create()?;
    fixtures::create_dir_all(root.path().join(".codex"))?;
    fixtures::write(root.path().join(".codex/config.toml"), "invalid TOML [")?;

    let error = load_installed(root.path(), PROVIDERS).unwrap_err();

    assert!(format!("{error:#}").contains(".codex/config.toml"));
    Ok(())
}

#[cfg(unix)]
#[test]
fn symlinks_below_a_directory_are_not_followed_by_its_file_list() -> Result<()> {
    let root = TemporaryDirectory::create()?;
    let outside = TemporaryDirectory::create()?;
    fixtures::write(outside.path().join("secret.json"), "{}")?;
    fixtures::write(root.path().join("real.json"), "{}")?;
    fixtures::symlink(outside.path(), root.path().join("linked-dir"))?;
    fixtures::symlink(
        root.path().join("real.json"),
        root.path().join("linked.json"),
    )?;
    fixtures::symlink(root.path().join("nowhere"), root.path().join("broken"))?;

    assert_eq!(regular_files(root.path())?, [root.path().join("real.json")]);
    assert_eq!(
        regular_files(&root.path().join("linked-dir"))?,
        [root.path().join("linked-dir/secret.json")]
    );
    assert_eq!(read_text_if_present(&root.path().join("broken"))?, None);
    Ok(())
}

#[cfg(unix)]
#[test]
fn a_symlink_met_inside_a_symlinked_directory_is_not_followed() -> Result<()> {
    let root = TemporaryDirectory::create()?;
    let shared = TemporaryDirectory::create()?;
    fixtures::write(shared.path().join("hook.sh"), "echo\n")?;
    fixtures::symlink(root.path(), shared.path().join("loop"))?;
    fixtures::symlink(shared.path(), root.path().join("hooks"))?;

    assert_eq!(
        regular_files(&root.path().join("hooks"))?,
        [root.path().join("hooks/hook.sh")]
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn a_looping_symlink_is_skipped_instead_of_failing_the_walk() -> Result<()> {
    let root = TemporaryDirectory::create()?;
    fixtures::symlink(root.path().join("loop"), root.path().join("loop"))?;
    fixtures::write(root.path().join("kept.json"), "{}")?;

    assert_eq!(regular_files(root.path())?, [root.path().join("kept.json")]);
    assert_eq!(read_text_if_present(&root.path().join("loop"))?, None);
    Ok(())
}

#[cfg(unix)]
#[test]
fn a_skills_support_files_skip_symlinks() -> Result<()> {
    let root = TemporaryDirectory::create()?;
    let outside = TemporaryDirectory::create()?;
    let skill = root.path().join("skills/one");
    fixtures::create_dir_all(&skill)?;
    fixtures::write(
        skill.join("SKILL.md"),
        "---\nname: one\ndescription: first\n---\n\n# One\n",
    )?;
    fixtures::write(skill.join("helper.txt"), "kept\n")?;
    fixtures::symlink(outside.path(), skill.join("escape"))?;

    let files = skill_files(skill.clone())?;

    assert_eq!(files.support.len(), 1);
    assert!(
        matches!(&files.support[0], SupportEntry::File { path, .. } if path == &skill.join("helper.txt"))
    );
    Ok(())
}

#[test]
fn a_bundle_that_maps_two_files_to_one_installed_path_is_refused_at_the_boundary() -> Result<()> {
    let root = TemporaryDirectory::create()?;
    let bundle_dir = TemporaryDirectory::create()?;
    fixtures::create_dir_all(bundle_dir.path().join(".codex/hooks"))?;
    fixtures::write(
        bundle_dir.path().join("hook_bundle.json"),
        r#"{"version": 1}"#,
    )?;
    fixtures::write(bundle_dir.path().join(".codex/foo.sh"), "one\n")?;
    fixtures::write(bundle_dir.path().join(".codex/hooks/foo.sh"), "two\n")?;
    let bundle: ExternalHookBundle = format!("tool={}", bundle_dir.path().display())
        .parse()
        .map_err(anyhow::Error::msg)?;
    let inputs = LoadInputs {
        hook_bundles: &[bundle],
        ..LoadInputs::new(PROVIDERS)
    };

    let error = fixtures::load_source_with(root.path(), &inputs).unwrap_err();

    assert!(format!("{error:#}").contains("maps two files to"));
    Ok(())
}

#[test]
fn a_bundle_manifest_with_an_unsupported_version_is_refused_with_its_path() -> Result<()> {
    let root = TemporaryDirectory::create()?;
    let bundle_dir = TemporaryDirectory::create()?;
    fixtures::write(
        bundle_dir.path().join("hook_bundle.json"),
        r#"{"version": 2}"#,
    )?;
    let bundle: ExternalHookBundle = format!("tool={}", bundle_dir.path().display())
        .parse()
        .map_err(anyhow::Error::msg)?;
    let inputs = LoadInputs {
        hook_bundles: &[bundle],
        ..LoadInputs::new(PROVIDERS)
    };

    let error = fixtures::load_source_with(root.path(), &inputs).unwrap_err();

    assert!(format!("{error:#}").contains("unsupported external hook bundle version"));
    Ok(())
}
