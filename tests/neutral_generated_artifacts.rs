use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;
use toml_edit::DocumentMut;

#[path = "neutral-generated-artifacts/config_sync.rs"]
mod config_sync;
#[path = "neutral-generated-artifacts/protection.rs"]
mod protection;
#[path = "neutral-generated-artifacts/provider_hooks.rs"]
mod provider_hooks;
#[path = "neutral-generated-artifacts/skills.rs"]
mod skills;

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
    assert!(prefix.join(".devin/hooks/hook_adapter.py").is_file());
    assert_runtime_hook_outputs(&prefix);
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
fn complete_source_uses_the_explicit_runtime_root() {
    let root = test_root("explicit-runtime-root");
    let project = root.join("project");

    run_harness([
        "install",
        "--source",
        complete_source_root().to_str().unwrap(),
        "--prefix",
        project.to_str().unwrap(),
        "--runtime-root",
        project.to_str().unwrap(),
    ]);

    let settings = read_json(&project.join(".claude/settings.json"));
    let codex_hooks = read_json(&project.join(".codex/hooks.json"));
    let policy = read_json(&project.join(".claude/hooks/rules/protected_paths.json"));
    let runtime_root = project.to_str().unwrap();

    assert_eq!(
        settings["hooks"]["PreToolUse"][0]["hooks"][0]["command"].as_str(),
        Some(
            format!("export AGENT_HARNESS_ROOT='{runtime_root}'; '{runtime_root}'/.claude/hooks/guard.sh")
                .as_str(),
        ),
    );
    assert_eq!(
        settings["statusLine"]["command"].as_str(),
        Some(
            format!(
                "export AGENT_HARNESS_ROOT='{runtime_root}'; '{runtime_root}'/.claude/statusline/statusline.sh"
            )
            .as_str(),
        ),
    );
    assert_eq!(
        codex_hooks["hooks"]["UserPromptSubmit"][0]["hooks"][0]["command"].as_str(),
        Some(
            format!(
                "export AGENT_HARNESS_ROOT='{runtime_root}'; '{runtime_root}'/.codex/hooks/adapt.sh '{runtime_root}'/.codex/hooks/guard.sh"
            )
            .as_str(),
        ),
    );
    assert!(json_array_contains(
        &policy["paths"],
        &format!("{runtime_root}/.claude/hooks/guard.sh"),
    ));

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
    assert!(json_array_contains(
        &settings["permissions"]["deny"],
        "Read(**/secrets/**)",
    ));
    assert!(json_array_contains(
        &settings["permissions"]["deny"],
        "Edit(**/secrets/**)",
    ));
    assert!(!json_array_contains(
        &settings["permissions"]["deny"],
        "Write(**/secrets/**)",
    ));
    assert!(rules.contains(r#"pattern = ["fixture","check"]"#));
    assert!(rules.contains(r#"decision = "allow""#));
    assert!(rules.contains(r#"decision = "prompt""#));
    assert!(rules.contains(r#"decision = "forbidden""#));

    remove_dir(root);
}

#[test]
fn complete_source_keeps_source_permission_rules_in_generated_claude_settings() {
    let root = test_root("source-permission-rules");
    let settings_path = root.join("settings.json");

    run_harness([
        "generate-claude-settings",
        "--source",
        complete_source_root().to_str().unwrap(),
        "--output",
        settings_path.to_str().unwrap(),
    ]);

    let settings = read_json(&settings_path);
    assert!(json_array_contains(
        &settings["permissions"]["ask"],
        "Bash(dangerouslyDisableSandbox:true)",
    ));
    assert!(json_array_contains(
        &settings["permissions"]["allow"],
        "Read"
    ));
    assert_eq!(settings["permissions"]["defaultMode"], "acceptEdits");

    remove_dir(root);
}

#[test]
fn installed_claude_settings_keep_source_ask_rules_after_sync() {
    let root = test_root("claude-settings-sync");
    let prefix = root.join("rendered");
    let installed_path = prefix.join(".claude/settings.json");
    let target_path = root.join("home/.claude/settings.json");

    run_harness([
        "install",
        "--source",
        complete_source_root().to_str().unwrap(),
        "--prefix",
        prefix.to_str().unwrap(),
    ]);
    std::fs::create_dir_all(target_path.parent().unwrap()).unwrap();
    std::fs::write(
        &target_path,
        r#"{"permissions": {"allow": ["Bash(stale:*)"], "ask": []}}"#,
    )
    .unwrap();
    run_harness([
        "sync-claude-settings",
        "--source",
        installed_path.to_str().unwrap(),
        "--target",
        target_path.to_str().unwrap(),
    ]);

    let installed = read_json(&installed_path);
    let target = read_json(&target_path);
    assert!(json_array_contains(
        &installed["permissions"]["ask"],
        "Bash(dangerouslyDisableSandbox:true)",
    ));
    assert!(json_array_contains(
        &target["permissions"]["ask"],
        "Bash(dangerouslyDisableSandbox:true)",
    ));

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

fn assert_runtime_hook_outputs(prefix: &Path) {
    let devin_config = read_json(&prefix.join(".config/devin/config.json"));
    assert_eq!(
        devin_config["hooks"]["PreToolUse"][0]["matcher"].as_str(),
        Some("exec"),
    );
    assert!(prefix.join(".hermes/hooks/hook_adapter.py").is_file());
    assert!(
        prefix
            .join(".hermes/plugins/agent-harness-hooks/plugin.yaml")
            .is_file()
    );
    assert!(
        prefix
            .join(".hermes/plugins/agent-harness-hooks/__init__.py")
            .is_file()
    );
    let hermes_manifest = read_json(&prefix.join(".hermes/hooks.json"));
    assert_eq!(
        hermes_manifest["hooks"]["pre_tool_call"][0]["matcher"].as_str(),
        Some("^(terminal|exec_command)$"),
    );
    assert!(prefix.join(".pi/hooks/hook_adapter.py").is_file());
    assert!(prefix.join(".pi/agent/extensions/hook_bridge.ts").is_file());
    let pi_manifest = read_json(&prefix.join(".pi/agent/hooks.json"));
    assert_eq!(
        pi_manifest["hooks"]["tool_call"][0]["matcher"].as_str(),
        Some("^bash$"),
    );
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
