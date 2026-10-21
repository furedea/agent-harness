use super::{
    assert_contains, complete_source_root, remove_dir, run_harness, run_harness_stdout, test_root,
};

#[test]
fn complete_source_inventory_reports_skills_and_hooks() {
    let skills = run_harness_stdout([
        "list",
        "skills",
        "--source",
        complete_source_root().to_str().unwrap(),
    ]);
    assert!(skills.starts_with("Skills (1)\n"));
    assert!(skills.contains("example-skill"));
    assert!(skills.contains("explicit"));

    let hooks = run_harness_stdout([
        "list",
        "hooks",
        "--source",
        complete_source_root().to_str().unwrap(),
    ]);
    assert!(hooks.contains("\nClaude\n"));
    assert!(hooks.contains("\nCodex\n"));
    assert!(hooks.contains("\nDevin\n"));
    assert!(hooks.contains("guard.sh"));
    assert!(hooks.contains("adapt.sh guard.sh"));
    assert!(hooks.contains("hook_adapter.py shell forbidden"));
}

#[test]
fn generate_skills_accepts_an_external_skill_directory() {
    let root = test_root("external-skill");
    let external_skill = root.join("upstream/external-tool");
    let output = root.join("output");
    std::fs::create_dir_all(external_skill.join("references")).unwrap();
    std::fs::write(
        external_skill.join("SKILL.md"),
        "---\nname: external-tool\ndescription: upstream skill\n---\n\n# External tool\n",
    )
    .unwrap();
    std::fs::write(
        external_skill.join("references/commands.md"),
        "Upstream commands\n",
    )
    .unwrap();
    let external_skill_arg = format!("external-tool={}", external_skill.display());

    run_harness([
        "generate-skills",
        "--source",
        complete_source_root().to_str().unwrap(),
        "--provider",
        "codex",
        "--extra-skill",
        &external_skill_arg,
        "--output",
        output.to_str().unwrap(),
    ]);

    assert_contains(
        &output.join("external-tool/SKILL.md"),
        "description: upstream skill",
    );
    assert_contains(
        &output.join("external-tool/references/commands.md"),
        "Upstream commands",
    );

    remove_dir(root);
}
