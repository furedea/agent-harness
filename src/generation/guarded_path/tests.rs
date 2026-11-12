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
    conflicts(entries, None)
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
fn double_star_matches_any_depth_and_classes_match_their_members() -> Result<()> {
    let entries = [
        deny("/w/**/.env*")?,
        deny("/w/[ab]x")?,
        read("/w/a/b/.env.local")?,
        read("/w/cx")?,
        read("/w/bx/f")?,
    ];

    assert_eq!(pairs(&entries), [(0, 2), (1, 4)]);
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
    assert!(GuardedPath::workspace(None).join("a/../../x").is_err());
}

#[test]
fn roots_do_not_cover_each_other_except_everything() -> Result<()> {
    let workspace = GuardedEntry::Deny(GuardedPath::workspace(None).join("secrets")?);
    let home = read("~/secrets/x")?;
    let everything = GuardedEntry::Deny(GuardedPath::everything());

    assert_eq!(pairs(&[workspace, home.clone()]), []);
    assert_eq!(pairs(&[everything, home]), [(0, 1)]);
    Ok(())
}

#[test]
fn braces_and_classes_match_only_their_alternatives() -> Result<()> {
    let entries = [
        deny("~/.{aws,ssh}/**")?,
        read("~/.aws/credentials")?,
        read("~/.codex/hooks.json")?,
        deny("/w/[a-c]?")?,
        read("/w/bz")?,
        read("/w/dz")?,
        deny("/v/[!a]x")?,
        read("/v/bx")?,
        read("/v/ax")?,
    ];

    assert_eq!(pairs(&entries), [(0, 1), (3, 4), (6, 7)]);
    Ok(())
}

#[test]
fn the_absolute_root_covers_home_entries() -> Result<()> {
    let entries = [
        deny("/")?,
        read("~/.codex/hooks.json")?,
        read("/etc/hosts")?,
    ];

    assert_eq!(pairs(&entries), [(0, 1), (0, 2)]);
    Ok(())
}

#[test]
fn a_backslash_separates_segments() -> Result<()> {
    let entries = [deny("C:\\work")?, read("C:/work/.codex/hooks.json")?];

    assert_eq!(pairs(&entries), [(0, 1)]);
    Ok(())
}

fn grant(path: &str, access: Access) -> Result<GuardedEntry> {
    GuardedEntry::new(GuardedPath::absolute(path)?, access)
        .ok_or_else(|| anyhow::anyhow!("{path} is not concrete"))
}

#[test]
fn a_looser_intermediate_entry_is_the_exception_for_the_entries_below_it() -> Result<()> {
    let entries = [
        GuardedEntry::Deny(GuardedPath::everything()),
        grant("~", Access::Write)?,
        read("~/.codex/hooks/adapt.sh")?,
    ];

    assert_eq!(pairs(&entries), [(0, 1)]);
    Ok(())
}

#[test]
fn a_stricter_intermediate_entry_is_the_one_that_conflicts() -> Result<()> {
    let entries = [
        GuardedEntry::Deny(GuardedPath::everything()),
        read("~")?,
        grant("~/x", Access::Write)?,
    ];

    assert_eq!(pairs(&entries), [(0, 1), (1, 2)]);
    Ok(())
}

#[test]
fn windows_drive_paths_compare_case_insensitively() -> Result<()> {
    let entries = [
        deny("C:/Work")?,
        read("c:/work/.codex/hooks.json")?,
        deny("C:/Proj/[A-Z]x")?,
        read("c:/proj/bx/f")?,
    ];

    assert_eq!(pairs(&entries), [(0, 1), (2, 3)]);
    Ok(())
}

#[test]
fn unix_paths_stay_case_sensitive() -> Result<()> {
    let entries = [deny("/Work")?, read("/work/x")?];

    assert_eq!(pairs(&entries), []);
    Ok(())
}

#[test]
fn a_backslash_is_a_filename_character_in_a_unix_path() -> Result<()> {
    let entries = [deny("/work/a")?, read("/work/a\\b")?, deny("/v/a\\b")?];

    assert_eq!(pairs(&entries), []);
    assert_ne!(
        GuardedPath::absolute("/w/a\\b")?,
        GuardedPath::absolute("/w/a/b")?
    );
    Ok(())
}

#[test]
fn a_backslash_separates_the_segments_of_a_unc_path() -> Result<()> {
    let entries = [deny("\\\\host\\share")?, read("\\\\host\\share\\x\\y")?];

    assert!(is_absolute_key("\\\\host\\share"));
    assert_eq!(pairs(&entries), [(0, 1)]);
    Ok(())
}

#[test]
fn a_trailing_hyphen_in_a_class_is_a_member() -> Result<()> {
    let entries = [
        deny("/w/[a-]x")?,
        read("/w/-x")?,
        read("/w/ax")?,
        read("/w/bx")?,
    ];

    assert_eq!(pairs(&entries), [(0, 1), (0, 2)]);
    Ok(())
}

#[test]
fn a_leading_hyphen_in_a_class_is_a_member() -> Result<()> {
    let entries = [deny("/w/[-a]x")?, read("/w/-x")?, read("/w/bx")?];

    assert_eq!(pairs(&entries), [(0, 1)]);
    Ok(())
}

#[test]
fn an_absolute_path_under_the_workspace_root_is_also_a_workspace_path() -> Result<()> {
    let root = GuardedPath::absolute("/work/proj")?;
    let secrets = GuardedEntry::Deny(GuardedPath::workspace(None).join("secrets/**")?);
    let hook = read("/work/proj/secrets/hook.sh")?;
    let outside = read("/work/other/secrets/hook.sh")?;

    let found = conflicts(&[secrets, hook, outside], Some(&root));

    let found: Vec<_> = found.iter().map(|c| (c.stricter, c.looser)).collect();
    assert_eq!(found, [(0, 1)]);
    Ok(())
}
