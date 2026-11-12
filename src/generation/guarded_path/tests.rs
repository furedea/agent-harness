use super::*;

fn deny(path: &str) -> Result<GuardedEntry> {
    Ok(GuardedEntry::Deny(GuardedPath::absolute(path)?))
}

fn read(path: &str) -> Result<GuardedEntry> {
    let entry = GuardedEntry::new(GuardedPath::absolute(path)?, Access::Read);
    entry.ok_or_else(|| anyhow::anyhow!("{path} is not concrete"))
}

/// The (stricter, looser) index pairs.
fn pairs(entries: &[GuardedEntry]) -> Vec<(usize, usize)> {
    conflicts(entries)
        .into_iter()
        .map(|conflict| (conflict.stricter, conflict.looser))
        .collect()
}

#[test]
fn a_looser_entry_cannot_be_built_from_a_pattern() -> Result<()> {
    let path = GuardedPath::absolute("~/.aws/*")?;

    assert!(GuardedEntry::new(path.clone(), Access::Read).is_none());
    assert!(GuardedEntry::new(path, Access::Deny).is_some());
    Ok(())
}

#[test]
fn an_ancestor_directory_covers_a_path_on_component_boundaries() -> Result<()> {
    let entries = [
        deny("~/.aws")?,
        read("~/.aws/credentials")?,
        read("~/.awsome/x")?,
    ];

    assert_eq!(pairs(&entries), [(0, 1)]);
    Ok(())
}

#[test]
fn an_equal_path_with_a_different_access_conflicts() -> Result<()> {
    let entries = [deny("~/.aws/credentials")?, read("~/.aws/credentials")?];

    assert_eq!(pairs(&entries), [(0, 1)]);
    Ok(())
}

#[test]
fn a_glob_covers_the_paths_it_matches() -> Result<()> {
    let entries = [
        deny("~/.aws/**")?,
        deny("/work/*/secret")?,
        read("~/.aws/credentials")?,
        read("/work/a/secret/key")?,
        read("/work/a/b/secret")?,
    ];

    assert_eq!(pairs(&entries), [(0, 2), (1, 3)]);
    Ok(())
}

#[test]
fn double_star_matches_any_depth_and_classes_are_widened() -> Result<()> {
    let entries = [
        deny("/w/**/.env*")?,
        deny("/w/[ab]x")?,
        read("/w/a/b/.env.local")?,
        read("/w/cx")?,
        read("/w/bx/f")?,
    ];

    // `[ab]` is widened to any one character, so `/w/cx` is covered too.
    assert_eq!(pairs(&entries), [(0, 2), (1, 3), (1, 4)]);
    Ok(())
}

#[test]
fn a_stricter_entry_below_a_looser_one_and_equal_accesses_do_not_conflict() -> Result<()> {
    let entries = [
        read("~/.config")?,
        deny("~/.config/gh")?,
        deny("~/.config/gh/hosts")?,
    ];

    assert_eq!(pairs(&entries), []);
    Ok(())
}

#[test]
fn paths_are_normalized_before_they_are_compared() -> Result<()> {
    let entries = [deny("~/.config/../.aws//")?, read("~/./.aws/credentials")?];

    assert_eq!(pairs(&entries), [(0, 1)]);
    Ok(())
}

#[test]
fn a_brace_group_spanning_a_slash_is_widened_to_everything_below() -> Result<()> {
    let entries = [
        deny("/w/{a/b,c}")?,
        read("/w/a/b")?,
        read("/w/z/deep")?,
        read("/v/a")?,
    ];

    assert_eq!(pairs(&entries), [(0, 1), (0, 2)]);
    Ok(())
}

#[test]
fn a_looser_subtree_entry_is_the_directory_and_a_drive_path_is_absolute() -> Result<()> {
    let subtree = GuardedEntry::new(GuardedPath::absolute("~/.ssh/**")?, Access::Read);
    let entries = [deny("~/.ssh/**")?, subtree.expect("a subtree is concrete")];

    assert_eq!(pairs(&entries), [(0, 1)]);
    assert!(GuardedPath::absolute("C:/proj/.claude/hooks/guard.sh").is_ok());
    assert!(is_absolute_key("C:/work") && !is_absolute_key("work/C:"));
    Ok(())
}

#[test]
fn an_explicit_exception_between_the_two_entries_resolves_the_looser_one() -> Result<()> {
    let entries = [deny("~")?, read("~/.codex")?, read("~/.codex/hooks.json")?];

    assert_eq!(pairs(&entries), [(0, 1)]);
    Ok(())
}

#[test]
fn a_path_cannot_climb_above_its_root() {
    assert!(GuardedPath::absolute("~/../x").is_err());
    assert!(GuardedPath::workspace().join("a/../../x").is_err());
}

#[test]
fn roots_do_not_cover_each_other_except_everything() -> Result<()> {
    let workspace = GuardedEntry::Deny(GuardedPath::workspace().join("secrets")?);
    let home = read("~/secrets/x")?;
    let everything = GuardedEntry::Deny(GuardedPath::everything());

    assert_eq!(pairs(&[workspace, home.clone()]), []);
    assert_eq!(pairs(&[everything, home]), [(0, 1)]);
    Ok(())
}
