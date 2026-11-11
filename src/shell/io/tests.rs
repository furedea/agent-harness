use std::os::unix::fs::{PermissionsExt, symlink};

use super::*;
use crate::shell::execute;

fn root(name: &str) -> Result<PathBuf> {
    let root = std::env::temp_dir().join(format!("agent-harness-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root)?;
    Ok(root)
}

fn mode_of(path: &Path) -> Result<u32> {
    Ok(std::fs::metadata(path)?.permissions().mode() & 0o777)
}

fn write_op(target: PathBuf, content: &str, mode: FileMode) -> FileOp {
    FileOp::WriteFile {
        target,
        content: content.as_bytes().to_vec(),
        mode,
    }
}

/// The mode a file gets when nothing asks for one: the process's default less its umask.
fn default_file_mode(root: &Path) -> Result<u32> {
    let probe = root.join("probe");
    std::fs::File::create(&probe)?;
    mode_of(&probe)
}

#[test]
fn a_write_preserves_an_unrelated_temporary_sibling() -> Result<()> {
    let root = root("io-sibling")?;
    let target = root.join("settings.json");
    let sibling = root.join("settings.tmp");
    std::fs::write(&sibling, "unrelated\n")?;

    execute(&[write_op(target.clone(), "installed\n", FileMode::Standard)])?;

    assert_eq!(std::fs::read_to_string(&sibling)?, "unrelated\n");
    assert_eq!(std::fs::read_to_string(&target)?, "installed\n");
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn a_write_replaces_a_symlink_with_a_regular_file_and_leaves_its_target() -> Result<()> {
    let root = root("io-symlink")?;
    let source = root.join("store/settings.json");
    let target = root.join("home/settings.json");
    std::fs::create_dir_all(root.join("store"))?;
    std::fs::create_dir_all(root.join("home"))?;
    std::fs::write(&source, "store\n")?;
    symlink(&source, &target)?;

    execute(&[write_op(target.clone(), "installed\n", FileMode::Standard)])?;

    assert!(std::fs::symlink_metadata(&target)?.file_type().is_file());
    assert_eq!(std::fs::read_to_string(&target)?, "installed\n");
    assert_eq!(std::fs::read_to_string(&source)?, "store\n");
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn only_private_files_are_owner_only() -> Result<()> {
    let root = root("io-modes")?;
    let default = default_file_mode(&root)?;

    execute(&[
        write_op(root.join("config.json"), "{}\n", FileMode::Standard),
        write_op(root.join("secret/token"), "token\n", FileMode::Private),
    ])?;

    assert_eq!(mode_of(&root.join("config.json"))?, default);
    assert_eq!(mode_of(&root.join("secret/token"))?, 0o600);
    assert_eq!(mode_of(&root.join("secret"))?, 0o700);
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn replacing_a_restricted_file_never_widens_it() -> Result<()> {
    let root = root("io-keep-mode")?;
    std::fs::write(root.join("config"), "old\n")?;
    std::fs::set_permissions(root.join("config"), std::fs::Permissions::from_mode(0o600))?;
    std::fs::write(root.join("generated"), "generated\n")?;

    execute(&[write_op(
        root.join("config"),
        "written\n",
        FileMode::Standard,
    )])?;
    assert_eq!(mode_of(&root.join("config"))?, 0o600);
    execute(&[FileOp::CopyFile {
        source: root.join("generated"),
        target: root.join("config"),
    }])?;

    assert_eq!(mode_of(&root.join("config"))?, 0o600);
    assert_eq!(std::fs::read_to_string(root.join("config"))?, "generated\n");
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn a_replaced_private_file_stays_private() -> Result<()> {
    let root = root("io-private-replace")?;
    std::fs::write(root.join("token"), "old\n")?;
    std::fs::set_permissions(root.join("token"), std::fs::Permissions::from_mode(0o644))?;

    execute(&[write_op(root.join("token"), "new\n", FileMode::Private)])?;

    assert_eq!(mode_of(&root.join("token"))?, 0o600);
    assert_eq!(std::fs::read_to_string(root.join("token"))?, "new\n");
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn explicit_bits_ignore_the_umask() -> Result<()> {
    let root = root("io-bits")?;

    execute(&[write_op(
        root.join("run.sh"),
        "#!/bin/sh\n",
        FileMode::Bits(0o755),
    )])?;

    assert_eq!(mode_of(&root.join("run.sh"))?, 0o755);
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn copies_keep_the_permission_bits_of_their_source() -> Result<()> {
    let root = root("io-copy-modes")?;
    std::fs::create_dir_all(root.join("source/nested"))?;
    std::fs::write(root.join("source/nested/run.sh"), "#!/bin/sh\n")?;
    std::fs::set_permissions(
        root.join("source/nested/run.sh"),
        std::fs::Permissions::from_mode(0o755),
    )?;

    execute(&[
        FileOp::CopyFile {
            source: root.join("source/nested/run.sh"),
            target: root.join("copied.sh"),
        },
        FileOp::CopyTree {
            source: root.join("source"),
            target: root.join("tree"),
        },
    ])?;

    assert_eq!(mode_of(&root.join("copied.sh"))?, 0o755);
    assert_eq!(mode_of(&root.join("tree/nested/run.sh"))?, 0o755);
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn a_tree_replacement_holds_exactly_its_contents() -> Result<()> {
    let root = root("io-replace-tree")?;
    std::fs::create_dir_all(root.join("skills/stale"))?;
    std::fs::write(root.join("skills/stale/SKILL.md"), "stale\n")?;
    std::fs::create_dir_all(root.join("external"))?;
    std::fs::write(root.join("external/notes.md"), "notes\n")?;

    execute(&[FileOp::ReplaceTree {
        target: root.join("skills"),
        contents: vec![
            write_op(
                root.join("skills/new/SKILL.md"),
                "new\n",
                FileMode::Standard,
            ),
            FileOp::CopyTree {
                source: root.join("external"),
                target: root.join("skills/new/extra"),
            },
        ],
    }])?;

    assert!(!root.join("skills/stale").exists());
    assert_eq!(
        std::fs::read_to_string(root.join("skills/new/SKILL.md"))?,
        "new\n"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("skills/new/extra/notes.md"))?,
        "notes\n"
    );
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn a_tree_replacement_may_be_empty() -> Result<()> {
    let root = root("io-empty-tree")?;

    execute(&[FileOp::ReplaceTree {
        target: root.join("optional"),
        contents: Vec::new(),
    }])?;

    assert!(root.join("optional").is_dir());
    assert_eq!(std::fs::read_dir(root.join("optional"))?.count(), 0);
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn a_tree_replacement_rejects_an_entry_outside_the_tree() -> Result<()> {
    let root = root("io-tree-outside")?;
    std::fs::create_dir_all(root.join("skills"))?;
    std::fs::write(root.join("skills/kept"), "kept\n")?;

    let outcome = execute(&[FileOp::ReplaceTree {
        target: root.join("skills"),
        contents: vec![write_op(root.join("elsewhere"), "x\n", FileMode::Standard)],
    }]);

    assert!(outcome.is_err());
    assert_eq!(std::fs::read_to_string(root.join("skills/kept"))?, "kept\n");
    assert_eq!(std::fs::read_dir(&root)?.count(), 1);
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn copying_a_missing_directory_keeps_the_target_and_its_new_parents_are_removed() -> Result<()> {
    let root = root("io-copy-missing")?;
    std::fs::create_dir_all(root.join("target"))?;
    std::fs::write(root.join("target/kept"), "kept\n")?;

    let kept = execute(&[FileOp::CopyTree {
        source: root.join("absent"),
        target: root.join("target"),
    }]);
    let created = execute(&[FileOp::CopyTree {
        source: root.join("absent"),
        target: root.join("new/parent/target"),
    }]);

    assert!(kept.is_err() && created.is_err());
    assert_eq!(std::fs::read_to_string(root.join("target/kept"))?, "kept\n");
    assert!(!root.join("new").exists());
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn restore_puts_a_directory_back_over_a_published_file() -> Result<()> {
    let root = root("io-restore-type-change")?;
    std::fs::create_dir_all(root.join("backup"))?;
    std::fs::write(root.join("backup/kept"), "kept\n")?;
    std::fs::write(root.join("target"), "published\n")?;

    restore(&root.join("target"), Some(&root.join("backup")))?;

    assert_eq!(std::fs::read_to_string(root.join("target/kept"))?, "kept\n");
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn replacing_a_directory_leaves_no_sibling_behind() -> Result<()> {
    let root = root("io-replace-leftovers")?;
    std::fs::create_dir_all(root.join("source"))?;
    std::fs::write(root.join("source/new"), "new\n")?;
    std::fs::create_dir_all(root.join("target"))?;
    std::fs::write(root.join("target/old"), "old\n")?;

    execute(&[FileOp::CopyTree {
        source: root.join("source"),
        target: root.join("target"),
    }])?;

    assert!(!root.join("target/old").exists());
    assert_eq!(std::fs::read_to_string(root.join("target/new"))?, "new\n");
    assert_eq!(std::fs::read_dir(&root)?.count(), 2);
    std::fs::remove_dir_all(root)?;
    Ok(())
}
