use std::path::{Path, PathBuf};

use super::*;
use crate::provider::spec::PROVIDERS;

#[test]
fn install_copies_each_managed_path_from_staging_to_the_install_root() {
    let rendered = InstalledLayout::new(Path::new("staging"));
    let installed = InstalledLayout::new(Path::new("home"));

    let plan = plan_install(&rendered, &installed);

    let expected_files = rendered
        .managed_files_for(PROVIDERS)
        .into_iter()
        .zip(installed.managed_files_for(PROVIDERS));
    for (source, target) in expected_files {
        assert!(plan.contains(&FileOp::CopyFile { source, target }));
    }
    let expected_directories = rendered
        .managed_directories_for(PROVIDERS)
        .into_iter()
        .zip(installed.managed_directories_for(PROVIDERS));
    for (source, target) in expected_directories {
        assert!(plan.contains(&FileOp::CopyTree { source, target }));
    }
}

#[test]
fn asset_plan_writes_every_provider_asset_at_its_output_relative_destination() {
    let plan = plan_assets_for(Path::new("home"), PROVIDERS);

    let targets: Vec<PathBuf> = plan.iter().map(|op| op.target().to_path_buf()).collect();
    for expected in [
        "home/.hermes/plugins/agent-harness-hooks/plugin.yaml",
        "home/.hermes/plugins/agent-harness-hooks/__init__.py",
        "home/.pi/agent/extensions/hook_bridge.ts",
        "home/.pi/agent/extensions/sandbox/file_operations.mjs",
        "home/.dsh/bridge/package.json",
        "home/.dsh/bridge/cordis.patch.yml",
        "home/.dsh/bridge/hook_bridge.ts",
    ] {
        assert!(targets.contains(&PathBuf::from(expected)));
    }
    assert_eq!(plan.len(), 9);
}

#[test]
fn planned_files_expand_a_copied_directory_into_its_listed_files() -> anyhow::Result<()> {
    let root = crate::shell::TemporaryDirectory::create()?;
    crate::shell::fixtures::create_dir_all(root.path().join("hooks/nested"))?;
    crate::shell::fixtures::write(root.path().join("hooks/a.sh"), "a")?;
    crate::shell::fixtures::write(root.path().join("hooks/nested/b.sh"), "b")?;
    let source = crate::shell::fixtures::load_source(root.path())?;
    let mut plan = InstallPlan::default();
    plan.push_stage(vec![
        FileOp::CopyTree {
            source: root.path().join("hooks"),
            target: PathBuf::from("out/.claude/hooks"),
        },
        FileOp::write("out/.claude/settings.json", "{}"),
    ]);

    let planned = plan.planned_files(&source);

    for expected in [
        "out/.claude/hooks/a.sh",
        "out/.claude/hooks/nested/b.sh",
        "out/.claude/settings.json",
    ] {
        assert!(planned.contains(&PathBuf::from(expected)), "{expected}");
    }
    Ok(())
}
