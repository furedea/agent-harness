use std::path::PathBuf;

use super::*;

fn test_root(name: &str) -> Result<PathBuf> {
    let root = std::env::temp_dir().join(format!("agent-harness-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root)?;
    Ok(root)
}

#[test]
fn a_failed_install_removes_the_parent_directories_it_created() -> Result<()> {
    let root = test_root("executor-created-parents")?;
    std::fs::write(root.join("rendered"), "rendered\n")?;
    let plan = vec![
        FileOp::CopyFile {
            source: root.join("rendered"),
            target: root.join("out/.claude/file"),
        },
        FileOp::CopyTree {
            source: root.join("absent"),
            target: root.join("out/.codex/hooks"),
        },
    ];

    assert!(execute(&plan).is_err());

    assert!(!root.join("out").exists());
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn replacing_a_file_never_leaves_it_missing_and_cleans_its_backup() -> Result<()> {
    let root = test_root("executor-file-replace")?;
    std::fs::write(root.join("rendered"), "new\n")?;
    std::fs::write(root.join("live"), "old\n")?;
    let plan = vec![FileOp::CopyFile {
        source: root.join("rendered"),
        target: root.join("live"),
    }];

    execute(&plan)?;

    assert_eq!(std::fs::read_to_string(root.join("live"))?, "new\n");
    assert_eq!(std::fs::read_dir(&root)?.count(), 2);
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn a_failed_install_keeps_an_existing_file_and_directory() -> Result<()> {
    let root = test_root("executor-keeps-live")?;
    std::fs::write(root.join("rendered"), "new\n")?;
    std::fs::write(root.join("live"), "old\n")?;
    std::fs::create_dir_all(root.join("hooks"))?;
    std::fs::write(root.join("hooks/kept"), "kept\n")?;
    let plan = vec![
        FileOp::CopyFile {
            source: root.join("rendered"),
            target: root.join("live"),
        },
        FileOp::CopyTree {
            source: root.join("absent"),
            target: root.join("hooks"),
        },
    ];

    assert!(execute(&plan).is_err());

    assert_eq!(std::fs::read_to_string(root.join("live"))?, "old\n");
    assert_eq!(std::fs::read_to_string(root.join("hooks/kept"))?, "kept\n");
    assert_eq!(std::fs::read_dir(&root)?.count(), 3);
    std::fs::remove_dir_all(root)?;
    Ok(())
}
