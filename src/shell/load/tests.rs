use super::*;
use crate::shell::{TemporaryDirectory, fixtures};

fn request(trees: &[&Path], documents: &[&Path]) -> SnapshotRequest {
    SnapshotRequest {
        trees: trees.iter().map(|path| path.to_path_buf()).collect(),
        documents: documents.iter().map(|path| path.to_path_buf()).collect(),
        ..SnapshotRequest::default()
    }
}

#[test]
fn a_walked_tree_records_text_only_for_document_extensions() -> Result<()> {
    let root = TemporaryDirectory::create()?;
    fixtures::create_dir_all(root.path().join("skills/one"))?;
    fixtures::write(root.path().join("skills/one/SKILL.md"), "# one\n")?;
    fixtures::write(root.path().join("run.sh"), "echo hi\n")?;

    let snapshot = load_snapshot(&request(&[root.path()], &[]))?;

    assert_eq!(
        snapshot.read_text(&root.path().join("skills/one/SKILL.md"))?,
        "# one\n"
    );
    assert!(snapshot.is_regular_file(&root.path().join("run.sh")));
    assert!(snapshot.read_text(&root.path().join("run.sh")).is_err());
    assert!(snapshot.is_dir(&root.path().join("skills")));
    Ok(())
}

#[test]
fn a_missing_tree_or_document_records_nothing() -> Result<()> {
    let root = TemporaryDirectory::create()?;
    let missing = root.path().join("missing");

    let snapshot = load_snapshot(&request(&[&missing], &[&missing.join("a.toml")]))?;

    assert!(!snapshot.exists(&missing));
    assert!(snapshot.regular_files(&missing).is_empty());
    Ok(())
}

#[test]
fn a_document_reads_any_utf8_file_and_leaves_binary_unreadable() -> Result<()> {
    let root = TemporaryDirectory::create()?;
    let config = root.path().join("config");
    let binary = root.path().join("binary");
    fixtures::write(&config, "key = 1\n")?;
    fixtures::write(&binary, [0xff, 0xfe])?;

    let snapshot = load_snapshot(&request(&[], &[&config, &binary]))?;

    assert_eq!(snapshot.read_text(&config)?, "key = 1\n");
    assert!(snapshot.read_text(&binary).is_err());
    Ok(())
}

#[cfg(unix)]
#[test]
fn symlinks_are_recorded_as_their_target_kind_and_skipped_by_a_listing_of_their_parent()
-> Result<()> {
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

    let snapshot = load_snapshot(&request(&[root.path()], &[]))?;

    assert!(snapshot.is_dir(&root.path().join("linked-dir")));
    assert!(snapshot.exists(&root.path().join("linked-dir/secret.json")));
    assert!(snapshot.is_file(&root.path().join("linked.json")));
    assert!(!snapshot.is_regular_file(&root.path().join("linked.json")));
    assert!(!snapshot.is_file(&root.path().join("broken")));
    assert!(snapshot.exists(&root.path().join("broken")));
    assert_eq!(
        snapshot.regular_files(root.path()),
        [root.path().join("real.json")]
    );
    assert_eq!(
        snapshot.regular_files(&root.path().join("linked-dir")),
        [root.path().join("linked-dir/secret.json")]
    );
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

    let snapshot = load_snapshot(&request(&[root.path()], &[]))?;

    assert!(snapshot.exists(&root.path().join("hooks/hook.sh")));
    assert!(snapshot.is_dir(&root.path().join("hooks/loop")));
    assert!(!snapshot.exists(&root.path().join("hooks/loop/hooks")));
    Ok(())
}

#[test]
fn a_probe_records_the_kind_without_reading_the_file() -> Result<()> {
    let root = TemporaryDirectory::create()?;
    let config = root.path().join("config.json");
    fixtures::write(&config, "{}")?;
    let request = SnapshotRequest {
        probes: vec![config.clone(), root.path().join("missing")],
        ..SnapshotRequest::default()
    };

    let snapshot = load_snapshot(&request)?;

    assert!(snapshot.is_file(&config));
    assert!(snapshot.read_text(&config).is_err());
    assert!(!snapshot.exists(&root.path().join("missing")));
    Ok(())
}

#[test]
fn a_listing_records_direct_entries_without_going_into_them() -> Result<()> {
    let root = TemporaryDirectory::create()?;
    fixtures::create_dir_all(root.path().join("nested"))?;
    fixtures::write(root.path().join("nested/deep.json"), "{}")?;
    fixtures::write(root.path().join("top.sh"), "echo\n")?;
    let request = SnapshotRequest {
        listings: vec![root.path().to_path_buf()],
        ..SnapshotRequest::default()
    };

    let snapshot = load_snapshot(&request)?;

    assert!(snapshot.is_dir(&root.path().join("nested")));
    assert!(snapshot.is_regular_file(&root.path().join("top.sh")));
    assert!(!snapshot.exists(&root.path().join("nested/deep.json")));
    Ok(())
}

#[test]
fn a_path_named_as_probe_and_document_keeps_its_text() -> Result<()> {
    let root = TemporaryDirectory::create()?;
    let config = root.path().join("manifest");
    fixtures::write(&config, "{}")?;
    let request = SnapshotRequest {
        documents: vec![config.clone()],
        probes: vec![config.clone()],
        trees: vec![root.path().to_path_buf()],
        ..SnapshotRequest::default()
    };

    let snapshot = load_snapshot(&request)?;

    assert_eq!(snapshot.read_text(&config)?, "{}");
    Ok(())
}

#[cfg(unix)]
#[test]
fn a_looping_symlink_is_recorded_as_other_instead_of_failing_the_walk() -> Result<()> {
    let root = TemporaryDirectory::create()?;
    fixtures::symlink(root.path().join("loop"), root.path().join("loop"))?;
    fixtures::write(root.path().join("kept.json"), "{}")?;

    let snapshot = load_snapshot(&request(&[root.path()], &[]))?;

    assert!(snapshot.exists(&root.path().join("loop")));
    assert!(!snapshot.is_file(&root.path().join("loop")));
    assert!(snapshot.is_file(&root.path().join("kept.json")));
    Ok(())
}
