use super::*;

fn file(text: &str) -> Entry {
    Entry::new(EntryKind::File, false).with_text(text.to_owned())
}

fn directory() -> Entry {
    Entry::new(EntryKind::Directory, false)
}

fn sample() -> Snapshot {
    let mut snapshot = Snapshot::default();
    snapshot.insert(PathBuf::from("/src"), directory());
    snapshot.insert(PathBuf::from("/src/a.json"), file("{}"));
    snapshot.insert(PathBuf::from("/src/skills"), directory());
    snapshot.insert(PathBuf::from("/src/skills/one"), directory());
    snapshot.insert(PathBuf::from("/src/skills/one/SKILL.md"), file("# one"));
    snapshot.insert(
        PathBuf::from("/src/skills/link"),
        Entry::new(EntryKind::Directory, true),
    );
    snapshot.insert(
        PathBuf::from("/src/skills/note.sh"),
        Entry::new(EntryKind::File, false),
    );
    snapshot.insert(
        PathBuf::from("/src/skills-extra"),
        Entry::new(EntryKind::Directory, false),
    );
    snapshot
}

#[test]
fn read_text_returns_loaded_content_and_names_the_missing_path() {
    let snapshot = sample();

    assert_eq!(snapshot.read_text(Path::new("/src/a.json")).unwrap(), "{}");
    let error = snapshot.read_text(Path::new("/src/b.json")).unwrap_err();
    assert!(error.to_string().contains("/src/b.json"));
}

#[test]
fn read_text_if_present_distinguishes_absent_from_unreadable() {
    let snapshot = sample();

    assert_eq!(
        snapshot
            .read_text_if_present(Path::new("/src/b.json"))
            .unwrap(),
        None
    );
    let error = snapshot
        .read_text_if_present(Path::new("/src/skills/note.sh"))
        .unwrap_err();
    assert!(error.to_string().contains("UTF-8 text"));
}

#[test]
fn children_list_direct_entries_only_and_not_siblings_sharing_a_prefix() {
    let snapshot = sample();

    let names: Vec<_> = snapshot
        .children(Path::new("/src/skills"))
        .into_iter()
        .map(|child| child.name.to_string_lossy().into_owned())
        .collect();

    assert_eq!(names, ["link", "note.sh", "one"]);
}

#[test]
fn symlinked_directories_are_directories_but_not_real_directories() {
    let snapshot = sample();
    let children = snapshot.children(Path::new("/src/skills"));
    let link = children.iter().find(|child| child.name == "link").unwrap();

    assert!(snapshot.is_dir(Path::new("/src/skills/link")));
    assert!(!link.is_real_directory());
    assert!(!snapshot.is_regular_file(Path::new("/src/skills/link")));
}

#[test]
fn regular_files_are_recursive_sorted_and_empty_for_a_missing_directory() {
    let snapshot = sample();

    assert_eq!(
        snapshot.regular_files(Path::new("/src/skills")),
        [
            PathBuf::from("/src/skills/note.sh"),
            PathBuf::from("/src/skills/one/SKILL.md"),
        ]
    );
    assert!(snapshot.regular_files(Path::new("/missing")).is_empty());
}
