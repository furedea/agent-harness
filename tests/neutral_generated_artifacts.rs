use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;
use toml_edit::DocumentMut;

#[test]
fn packaged_source_defaults_to_minimal() {
    let root = test_root("packaged-minimal");
    let cwd = root.join("cwd");
    let prefix = root.join("home");
    std::fs::create_dir_all(&cwd).unwrap();

    let packaged = run_harness_in_stdout(&cwd, ["list", "skills"]);
    let explicit = run_harness_stdout([
        "list",
        "skills",
        "--source",
        minimal_source_root().to_str().unwrap(),
    ]);
    assert_eq!(packaged, explicit);
    assert!(packaged.starts_with("Skills (0)\n"));

    run_harness_in(&cwd, ["install", "--prefix", prefix.to_str().unwrap()]);
    assert!(prefix.join(".codex/AGENTS.md").is_file());
    assert!(prefix.join(".claude/CLAUDE.md").is_file());
    assert_eq!(
        std::fs::read_dir(prefix.join(".codex/skills"))
            .unwrap()
            .count(),
        0,
    );

    remove_dir(root);
}

#[test]
fn complete_source_installs_provider_outputs() {
    let root = test_root("complete-source-install");
    let prefix = root.join("home");

    run_harness([
        "install",
        "--source",
        complete_source_root().to_str().unwrap(),
        "--prefix",
        prefix.to_str().unwrap(),
    ]);

    assert_eq!(
        std::fs::read(prefix.join(".codex/AGENTS.md")).unwrap(),
        std::fs::read(complete_source_root().join("AGENTS.md")).unwrap(),
    );
    assert!(prefix.join(".claude/hooks/guard.sh").is_file());
    assert!(prefix.join(".codex/hooks/adapt.sh").is_file());
    assert!(prefix.join(".codex/hooks/guard.sh").is_file());
    assert_contains(
        &prefix.join(".claude/skills/example-skill/SKILL.md"),
        "disable-model-invocation: true",
    );
    assert_contains(
        &prefix.join(".codex/skills/example-skill/agents/openai.yaml"),
        "allow_implicit_invocation: false",
    );

    let settings = read_json(&prefix.join(".claude/settings.json"));
    let config = read_toml(&prefix.join(".codex/config.toml"));
    assert_eq!(settings["model"], "fixture-claude");
    assert_eq!(config["model"].as_str(), Some("fixture-codex"));

    remove_dir(root);
}

#[test]
fn complete_source_generates_shared_command_permissions() {
    let root = test_root("command-permissions");
    let settings_path = root.join("settings.json");
    let rules_path = root.join("default.rules");

    run_harness([
        "generate-claude-settings",
        "--source",
        complete_source_root().to_str().unwrap(),
        "--output",
        settings_path.to_str().unwrap(),
    ]);
    run_harness([
        "generate-codex-rules",
        "--source",
        complete_source_root().to_str().unwrap(),
        "--output",
        rules_path.to_str().unwrap(),
    ]);

    let settings = read_json(&settings_path);
    let rules = std::fs::read_to_string(&rules_path).unwrap();
    assert!(json_array_contains(
        &settings["permissions"]["allow"],
        "Bash(fixture check:*)",
    ));
    assert!(json_array_contains(
        &settings["permissions"]["ask"],
        "Bash(fixture publish:*)",
    ));
    assert!(json_array_contains(
        &settings["permissions"]["deny"],
        "Bash(fixture destroy:*)",
    ));
    assert!(rules.contains(r#"pattern = ["fixture","check"]"#));
    assert!(rules.contains(r#"decision = "allow""#));
    assert!(rules.contains(r#"decision = "prompt""#));
    assert!(rules.contains(r#"decision = "forbidden""#));

    remove_dir(root);
}

#[test]
fn complete_source_keeps_protection_layers_aligned() {
    let root = test_root("protected-paths");
    let prefix = root.join("home");

    run_harness([
        "install",
        "--source",
        complete_source_root().to_str().unwrap(),
        "--prefix",
        prefix.to_str().unwrap(),
    ]);

    let policy = read_json(&prefix.join(".claude/hooks/rules/protected_paths.json"));
    let settings = read_json(&prefix.join(".claude/settings.json"));
    let policy_paths = string_set(&policy["paths"]);
    let deny_write = string_set(&settings["sandbox"]["filesystem"]["denyWrite"]);

    assert_eq!(policy["version"], 1);
    assert_eq!(policy_paths, deny_write);
    assert!(policy_paths.contains("~/.claude/hooks/guard.sh"));
    assert!(policy_paths.contains("~/.codex/hooks/adapt.sh"));
    assert!(!policy_paths.iter().any(|path| path.starts_with('/')));

    remove_dir(root);
}

#[test]
fn complete_source_verify_checks_declared_runtime_commands() {
    let root = test_root("runtime-commands");
    let prefix = root.join("home");
    let bin = root.join("bin");
    std::fs::create_dir_all(&bin).unwrap();

    run_harness([
        "install",
        "--source",
        complete_source_root().to_str().unwrap(),
        "--prefix",
        prefix.to_str().unwrap(),
    ]);

    let missing = verify_complete_source(&prefix, Path::new(""));
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("fixture-runtime"));

    write_executable(&bin.join("fixture-runtime"));
    let available = verify_complete_source(&prefix, &bin);
    assert!(
        available.status.success(),
        "verify failed: {}",
        String::from_utf8_lossy(&available.stderr),
    );

    remove_dir(root);
}

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
    assert!(hooks.contains("guard.sh"));
    assert!(hooks.contains("adapt.sh guard.sh"));
}

#[test]
fn codex_config_sync_preserves_codex_owned_state() {
    let root = test_root("codex-sync");
    let source_path = root.join("source.toml");
    let target_path = root.join("target.toml");

    std::fs::write(&source_path, "model = \"source-model\"\n").unwrap();
    std::fs::write(
        &target_path,
        r#"
model = "target-model"
sandbox_mode = "read-only"

[projects."/tmp/project"]
trust_level = "trusted"
"#,
    )
    .unwrap();

    run_harness([
        "sync-codex-config",
        "--source",
        source_path.to_str().unwrap(),
        "--target",
        target_path.to_str().unwrap(),
    ]);

    let target = read_toml(&target_path);
    assert_eq!(target["model"].as_str(), Some("source-model"));
    assert!(target.get("sandbox_mode").is_none());
    assert_eq!(
        target["projects"]["/tmp/project"]["trust_level"].as_str(),
        Some("trusted"),
    );

    remove_dir(root);
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

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn minimal_source_root() -> PathBuf {
    repo_root().join("profiles/minimal")
}

fn complete_source_root() -> PathBuf {
    repo_root().join("tests/fixtures/complete-source")
}

fn test_root(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "agent-harness-generator-{name}-{}-{nanos}",
        std::process::id(),
    ));
    std::fs::create_dir_all(&root).unwrap();
    root
}

fn remove_dir(path: PathBuf) {
    std::fs::remove_dir_all(path).unwrap();
}

fn write_executable(path: &Path) {
    std::fs::write(path, "#!/bin/sh\nexit 0\n").unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}

fn verify_complete_source(prefix: &Path, path: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_agent-harness"))
        .env("PATH", path)
        .args([
            "verify",
            "--source",
            complete_source_root().to_str().unwrap(),
            "--prefix",
            prefix.to_str().unwrap(),
        ])
        .output()
        .unwrap()
}

fn run_harness<const N: usize>(args: [&str; N]) {
    run_harness_command(Command::new(env!("CARGO_BIN_EXE_agent-harness")).args(args));
}

fn run_harness_stdout<const N: usize>(args: [&str; N]) -> String {
    let output = run_harness_command(Command::new(env!("CARGO_BIN_EXE_agent-harness")).args(args));
    String::from_utf8(output.stdout).unwrap()
}

fn run_harness_in<const N: usize>(cwd: &Path, args: [&str; N]) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_agent-harness"));
    command
        .current_dir(cwd)
        .env_remove("AGENT_HARNESS_SOURCE")
        .args(args);
    run_harness_command(&mut command);
}

fn run_harness_in_stdout<const N: usize>(cwd: &Path, args: [&str; N]) -> String {
    let mut command = Command::new(env!("CARGO_BIN_EXE_agent-harness"));
    let output = run_harness_command(
        command
            .current_dir(cwd)
            .env_remove("AGENT_HARNESS_SOURCE")
            .args(args),
    );
    String::from_utf8(output.stdout).unwrap()
}

fn run_harness_command(command: &mut Command) -> std::process::Output {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "agent-harness failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    output
}

fn read_json(path: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn read_toml(path: &Path) -> DocumentMut {
    std::fs::read_to_string(path)
        .unwrap()
        .parse::<DocumentMut>()
        .unwrap()
}

fn string_set(value: &Value) -> BTreeSet<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item.as_str().unwrap().to_owned())
        .collect()
}

fn json_array_contains(value: &Value, expected: &str) -> bool {
    value
        .as_array()
        .is_some_and(|items| items.iter().any(|item| item.as_str() == Some(expected)))
}

fn assert_contains(path: &Path, expected: &str) {
    let content = std::fs::read_to_string(path).unwrap();
    assert!(
        content.contains(expected),
        "{} did not contain {expected:?}",
        path.display(),
    );
}
