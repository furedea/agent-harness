use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;
use toml_edit::DocumentMut;

#[test]
fn claude_settings_render_from_real_source() {
    let root = test_root("claude-settings");
    let settings_path = root.join("settings.json");

    run_harness([
        "generate-claude-settings",
        "--source",
        repo_root().to_str().unwrap(),
        "--output",
        settings_path.to_str().unwrap(),
    ]);

    let generated = read_json(&settings_path);
    let base = read_json(&repo_root().join("claude/settings.base.json"));
    let base_deny = permission_set(&base, "deny");

    assert_eq!(generated["model"], base["model"]);
    assert_eq!(generated["language"], base["language"]);
    assert!(base.get("hooks").is_none());
    assert!(
        !generated["hooks"]["PreToolUse"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    assert_eq!(
        non_generated_permissions(&generated, "allow"),
        non_generated_permissions(&base, "allow"),
    );
    assert!(!base_deny.contains("Read(.env*)"));
    assert!(!base_deny.contains("Write(.env*)"));

    let allow = permission_set(&generated, "allow");
    let deny = permission_set(&generated, "deny");
    assert!(allow.contains("Bash(uv run:*)"));
    assert!(deny.contains("Bash(rm:*)"));
    assert!(deny.contains("Bash(brew install:*)"));
    assert!(deny.contains("Read(.env*)"));
    assert!(deny.contains("Read(~/.docker/config.json)"));
    assert!(deny.contains("Write(**/secrets/**)"));

    assert_eq!(
        generated["sandbox"]["filesystem"]["allowWrite"],
        base["sandbox"]["filesystem"]["allowWrite"],
    );
    let deny_write = generated["sandbox"]["filesystem"]["denyWrite"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap().to_owned())
        .collect::<BTreeSet<_>>();
    assert!(deny_write.contains("~/.claude/hooks/guard_allowed_commands.sh"));
    assert!(deny_write.contains("~/.claude/hooks/rules/allowed_commands.json"));
    assert!(deny_write.contains("~/.claude/hooks/rules/command_policy.json"));
    assert!(deny_write.contains("~/.claude/hooks/rules/forbidden_commands.json"));
    assert!(deny_write.contains("~/.claude/hooks/rules/protected_paths.json"));
    assert!(deny_write.contains("~/.claude/hooks/rules/secret_commit_policy.json"));
    assert!(!deny_write.contains("~/.claude/rules/forbidden_commands.json"));
    assert!(deny_write.contains("~/.codex/hooks/adapt_shell_command.sh"));
    assert!(!deny_write.iter().any(|path| path.starts_with('/')));
    assert!(!deny_write.iter().any(|path| path.contains("/skills/")));

    assert_all_hook_commands_resolve(&generated);
    assert!(
        generated["hooks"]["PreToolUse"]
            .as_array()
            .unwrap()
            .iter()
            .any(|group| {
                group["matcher"] == "Write|Edit|MultiEdit"
                    && group["hooks"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|hook| hook["command"] == "$HOME/.claude/hooks/guard_harness_files.sh")
            })
    );

    remove_dir(root);
}

#[test]
fn claude_settings_render_from_packaged_source_without_source_argument() {
    let root = test_root("packaged-source");
    let cwd = root.join("cwd");
    let settings_path = root.join("settings.json");
    std::fs::create_dir_all(&cwd).unwrap();

    run_harness_in(
        &cwd,
        [
            "generate-claude-settings",
            "--output",
            settings_path.to_str().unwrap(),
        ],
    );

    let generated = read_json(&settings_path);
    assert!(
        !generated["hooks"]["PreToolUse"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    remove_dir(root);
}

#[test]
fn list_skills_uses_packaged_source_without_source_argument() {
    let root = test_root("packaged-list");
    let cwd = root.join("cwd");
    std::fs::create_dir_all(&cwd).unwrap();

    let packaged = run_harness_in_stdout(&cwd, ["list", "skills"]);
    let explicit =
        run_harness_stdout(["list", "skills", "--source", repo_root().to_str().unwrap()]);

    assert_eq!(packaged, explicit);

    remove_dir(root);
}

#[test]
fn install_uses_packaged_source_without_source_argument() {
    let root = test_root("packaged-install");
    let cwd = root.join("cwd");
    let prefix = root.join("home");
    std::fs::create_dir_all(&cwd).unwrap();

    run_harness_in(&cwd, ["install", "--prefix", prefix.to_str().unwrap()]);

    assert_eq!(
        std::fs::read(prefix.join(".codex/AGENTS.md")).unwrap(),
        std::fs::read(repo_root().join("agents/AGENTS.md")).unwrap(),
    );
    assert!(prefix.join(".codex/hooks/adapt_shell_command.sh").is_file());
    assert!(
        prefix
            .join(".codex/hooks/adapt_guard_secret_paths.sh")
            .is_file()
    );
    assert!(
        prefix
            .join(".claude/hooks/rules/allowed_commands.json")
            .is_file()
    );
    assert!(
        prefix
            .join(".claude/hooks/rules/command_policy.json")
            .is_file()
    );
    assert!(
        prefix
            .join(".claude/hooks/rules/forbidden_commands.json")
            .is_file()
    );
    assert!(
        prefix
            .join(".claude/hooks/rules/protected_paths.json")
            .is_file()
    );
    assert!(
        prefix
            .join(".claude/hooks/rules/secret_commit_policy.json")
            .is_file()
    );
    assert!(
        prefix
            .join(".claude/hooks/rules/secret_path_policy.json")
            .is_file()
    );
    assert!(prefix.join(".claude/settings.json").is_file());

    remove_dir(root);
}

#[test]
fn install_keeps_runtime_and_claude_protected_paths_aligned() {
    let root = test_root("protected-paths");
    let prefix = root.join("home");

    run_harness([
        "install",
        "--source",
        repo_root().to_str().unwrap(),
        "--prefix",
        prefix.to_str().unwrap(),
    ]);

    let policy = read_json(&prefix.join(".claude/hooks/rules/protected_paths.json"));
    let policy_paths = policy["paths"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap().to_owned())
        .collect::<BTreeSet<_>>();
    let settings = read_json(&prefix.join(".claude/settings.json"));
    let deny_write = settings["sandbox"]["filesystem"]["denyWrite"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap().to_owned())
        .collect::<BTreeSet<_>>();

    assert_eq!(policy["version"], 1);
    assert_eq!(policy_paths, deny_write);

    remove_dir(root);
}

#[test]
fn codex_config_outputs_render_from_real_source() {
    let root = test_root("codex-config");
    let fragment_path = root.join("fragment.toml");
    let source_path = root.join("source.toml");
    let target_path = root.join("target.toml");

    run_harness([
        "generate-codex-config-fragment",
        "--source",
        repo_root().to_str().unwrap(),
        "--output",
        fragment_path.to_str().unwrap(),
    ]);
    let fragment = read_toml(&fragment_path);
    let filesystem = fragment["permissions"]["guarded"]["filesystem"]
        .as_table()
        .unwrap();
    assert_eq!(filesystem["glob_scan_max_depth"].as_integer(), Some(5));
    assert_eq!(
        filesystem["~/.claude/hooks/guard_allowed_commands.sh"].as_str(),
        Some("read"),
    );
    assert_eq!(
        filesystem["~/.claude/hooks/rules/allowed_commands.json"].as_str(),
        Some("read"),
    );
    assert_eq!(
        filesystem["~/.claude/hooks/rules/command_policy.json"].as_str(),
        Some("read"),
    );
    assert_eq!(
        filesystem["~/.claude/hooks/rules/forbidden_commands.json"].as_str(),
        Some("read"),
    );
    assert!(
        filesystem
            .get("~/.claude/rules/forbidden_commands.json")
            .is_none()
    );
    assert_eq!(
        filesystem["~/.codex/hooks/adapt_shell_command.sh"].as_str(),
        Some("read"),
    );
    assert!(!filesystem.iter().any(|(key, _)| key.starts_with('/')));
    assert!(!filesystem.iter().any(|(key, _)| key.contains("/skills/")));
    assert!(
        filesystem
            .iter()
            .filter(|(key, _)| *key != "glob_scan_max_depth")
            .all(|(_, item)| item.as_str() == Some("read"))
    );

    let base_config = read_toml(&repo_root().join("codex/config.toml"));
    assert!(base_config.get("default_permissions").is_none());

    run_harness([
        "generate-codex-config-source",
        "--source",
        repo_root().to_str().unwrap(),
        "--output",
        source_path.to_str().unwrap(),
    ]);
    std::fs::write(&target_path, "default_permissions = \"guarded\"\n").unwrap();
    run_harness([
        "sync-codex-config",
        "--source",
        source_path.to_str().unwrap(),
        "--target",
        target_path.to_str().unwrap(),
    ]);
    let target = read_toml(&target_path);
    assert!(target.get("default_permissions").is_none());
    assert_eq!(
        target["permissions"]["guarded"]["filesystem"]["~/.codex/hooks/adapt_shell_command.sh"]
            .as_str(),
        Some("read"),
    );

    remove_dir(root);
}

#[test]
fn codex_config_sync_preserves_codex_owned_state() {
    let root = test_root("codex-sync");
    let source_path = root.join("source.toml");
    let target_path = root.join("target.toml");

    std::fs::write(
        &source_path,
        r#"
model = "gpt-5.5"

[features]
hooks = true
"#,
    )
    .unwrap();
    std::fs::write(
        &target_path,
        r#"
model = "gpt-5.4"
sandbox_mode = "read-only"

[projects."/Users/kaito/project"]
trust_level = "trusted"

[marketplaces.openai-bundled]
last_updated = "2026-04-27T09:45:32Z"
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
    assert_eq!(target["model"].as_str(), Some("gpt-5.5"));
    assert!(target.get("sandbox_mode").is_none());
    assert_eq!(
        target["projects"]["/Users/kaito/project"]["trust_level"].as_str(),
        Some("trusted"),
    );
    assert_eq!(
        target["marketplaces"]["openai-bundled"]["last_updated"].as_str(),
        Some("2026-04-27T09:45:32Z"),
    );

    remove_dir(root);
}

#[test]
fn codex_hooks_render_from_real_source() {
    let root = test_root("codex-hooks");
    let hooks_path = root.join("hooks.json");

    run_harness([
        "generate-codex-hooks",
        "--source",
        repo_root().to_str().unwrap(),
        "--output",
        hooks_path.to_str().unwrap(),
    ]);

    let hooks = read_json(&hooks_path);
    assert_all_hook_commands_resolve(&hooks);
    assert_no_duplicate_commands_per_codex_group(&hooks);
    assert!(
        hooks["hooks"]["PreToolUse"]
            .as_array()
            .unwrap()
            .iter()
            .any(|group| {
                group["matcher"].as_str().unwrap().contains("Bash")
                    && group["hooks"].as_array().unwrap().iter().any(|hook| {
                        hook["command"]
                            .as_str()
                            .unwrap()
                            .starts_with("$HOME/.codex/hooks/adapt_shell_command.sh ")
                    })
            })
    );
    assert!(
        hooks["hooks"]["PreToolUse"]
            .as_array()
            .unwrap()
            .iter()
            .any(|group| {
                group["matcher"].as_str().unwrap().contains("Bash")
                    && group["hooks"].as_array().unwrap().iter().any(|hook| {
                        hook["command"].as_str().unwrap()
                            == "$HOME/.codex/hooks/adapt_guard_secret_paths.sh command"
                    })
            })
    );
    assert!(
        hooks["hooks"]["PreToolUse"]
            .as_array()
            .unwrap()
            .iter()
            .any(|group| {
                group["matcher"].as_str().unwrap().contains("apply_patch")
                    && group["hooks"].as_array().unwrap().iter().any(|hook| {
                        hook["command"].as_str().unwrap()
                            == "$HOME/.codex/hooks/adapt_guard_secret_paths.sh patch"
                    })
            })
    );

    remove_dir(root);
}

#[test]
fn command_policy_outputs_render_from_real_source() {
    let root = test_root("command-policy");
    let rules_path = root.join("default.rules");
    let runtime_path = root.join("command-policy.json");
    let forbidden_path = root.join("forbidden.json");
    let settings_path = root.join("settings.json");

    run_harness([
        "generate-codex-rules",
        "--source",
        repo_root().to_str().unwrap(),
        "--output",
        rules_path.to_str().unwrap(),
    ]);
    let rules = std::fs::read_to_string(&rules_path).unwrap();
    assert!(rules.contains(r#"decision = "allow""#));
    assert!(rules.contains(r#"decision = "forbidden""#));
    assert!(rules.contains(r#"pattern = ["uv","run"]"#));
    assert!(rules.contains(r#"pattern = ["rm"]"#));
    assert!(rules.contains(r#"pattern = ["brew","install"]"#));

    run_harness([
        "generate-command-policy",
        "--source",
        repo_root().to_str().unwrap(),
        "--output",
        runtime_path.to_str().unwrap(),
    ]);
    let runtime = read_json(&runtime_path);
    assert!(runtime["rules"].as_array().unwrap().iter().any(|rule| {
        rule["decision"] == "allow" && rule["pattern"] == serde_json::json!(["uv", "run"])
    }));
    assert!(runtime["rules"].as_array().unwrap().iter().any(|rule| {
        rule["decision"] == "forbidden" && rule["pattern"] == serde_json::json!(["rm"])
    }));

    run_harness([
        "generate-forbidden-commands",
        "--source",
        repo_root().to_str().unwrap(),
        "--output",
        forbidden_path.to_str().unwrap(),
    ]);
    let forbidden = read_json(&forbidden_path);
    assert!(forbidden["rules"].as_array().unwrap().iter().any(|rule| {
        rule["patterns"].as_array().unwrap().iter().any(|pattern| {
            pattern == "^git[[:space:]]+add[[:space:]]+(\\.|-A|--all)([[:space:]]|$)"
        })
    }));

    run_harness([
        "generate-claude-settings",
        "--source",
        repo_root().to_str().unwrap(),
        "--output",
        settings_path.to_str().unwrap(),
    ]);
    let settings = read_json(&settings_path);
    let policy = read_json(&repo_root().join("agents/command_policy.json"));
    assert_policy_covers_permissions(&policy, &settings, "allow");
    assert_policy_covers_permissions(&policy, &settings, "deny");

    remove_dir(root);
}

#[test]
fn skills_render_from_real_source() {
    let root = test_root("skills");
    let claude_out = root.join("claude");
    let codex_out = root.join("codex");

    assert_source_skill_frontmatter_is_common();

    run_harness([
        "generate-skills",
        "--source",
        repo_root().to_str().unwrap(),
        "--provider",
        "claude",
        "--output",
        claude_out.to_str().unwrap(),
    ]);
    run_harness([
        "generate-skills",
        "--source",
        repo_root().to_str().unwrap(),
        "--provider",
        "codex",
        "--output",
        codex_out.to_str().unwrap(),
    ]);

    assert_contains(
        &claude_out.join("git-commit-split/SKILL.md"),
        "argument-hint: \"{direct | pr-per-feature}\"",
    );
    assert_contains(
        &codex_out.join("git-commit-split/SKILL.md"),
        "argument-hint: \"{direct | pr-per-feature}\"",
    );
    assert_contains(
        &claude_out.join("skill-auditor/SKILL.md"),
        "disable-model-invocation: true",
    );

    for skill in [
        "git-commit-split",
        "github-ci-init",
        "nix-dev-init",
        "skill-auditor",
    ] {
        assert_contains(
            &claude_out.join(format!("{skill}/SKILL.md")),
            "disable-model-invocation: true",
        );
        assert_contains(
            &codex_out.join(format!("{skill}/agents/openai.yaml")),
            "allow_implicit_invocation: false",
        );
        assert!(
            !claude_out
                .join(format!("{skill}/agents/openai.yaml"))
                .exists()
        );
    }

    remove_dir(root);
}

#[test]
fn list_skills_reports_fixture_metadata_in_name_order() {
    let root = test_root("list-skills");
    write_inventory_source(&root);

    let stdout = run_harness_stdout(["list", "skills", "--source", root.to_str().unwrap()]);
    let rows = stdout
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| line.split_whitespace().collect::<Vec<_>>())
        .collect::<Vec<_>>();

    assert_eq!(rows[0], ["Skills", "(2)"]);
    assert_eq!(rows[1], ["NAME", "TITLE", "CLAUDE", "CODEX"]);
    assert_eq!(rows[2], ["alpha", "Alpha", "Skill", "implicit", "implicit"]);
    assert_eq!(rows[3], ["zeta", "Zeta", "Skill", "explicit", "explicit"]);

    remove_dir(root);
}

#[test]
fn list_hooks_renders_grouped_fixture_metadata() {
    let root = test_root("list-hooks");
    write_inventory_source(&root);

    let stdout = run_harness_stdout(["list", "hooks", "--source", root.to_str().unwrap()]);
    let expected = format!(
        concat!(
            "Hooks\n\n",
            "Claude\n\n",
            "PreToolUse\n",
            "  matcher: Bash | Edit\n",
            "    guard.sh\n",
            "    {:<29}when: *.js, *.ts\n\n",
            "Codex\n\n",
            "UserPromptSubmit\n",
            "  matcher: any\n",
            "    adapt_shell_command.sh -> guard.sh\n",
        ),
        "lint.sh",
    );

    assert_eq!(stdout, expected);

    remove_dir(root);
}

#[test]
fn list_hooks_filters_entries_by_provider() {
    let stdout = run_harness_stdout([
        "list",
        "hooks",
        "--provider",
        "codex",
        "--source",
        repo_root().to_str().unwrap(),
    ]);

    assert!(stdout.contains("\nCodex\n"));
    assert!(!stdout.contains("\nClaude\n"));
}

#[test]
fn list_summarizes_skills_and_hook_events() {
    let stdout = run_harness_stdout(["list", "--source", repo_root().to_str().unwrap()]);
    let hooks = read_json(&repo_root().join("agents/hooks.json"));
    let skill_count = std::fs::read_dir(repo_root().join("agents/skills"))
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.path().join("SKILL.md").is_file())
        .count();
    let expected = format!(
        "Agent Harness inventory\n\n\
         Skills                  {skill_count}\n\
         Claude hook events      {}\n\
         Codex hook events       {}\n",
        hooks["claude"].as_object().unwrap().len(),
        hooks["codex"]["hooks"].as_object().unwrap().len(),
    );

    assert_eq!(stdout, expected);
}

#[test]
fn generate_skills_accepts_an_external_skill_directory() {
    let root = test_root("external-skill");
    let external_skill = root.join("upstream/external-tool");
    let out = root.join("out");
    std::fs::create_dir_all(external_skill.join("references")).unwrap();
    std::fs::write(
        external_skill.join("SKILL.md"),
        "---\nname: external-tool\ndescription: upstream skill\n---\n\nBody\n",
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
        repo_root().to_str().unwrap(),
        "--provider",
        "codex",
        "--extra-skill",
        &external_skill_arg,
        "--output",
        out.to_str().unwrap(),
    ]);

    assert_contains(
        &out.join("external-tool/SKILL.md"),
        "description: upstream skill",
    );
    assert_contains(
        &out.join("external-tool/references/commands.md"),
        "Upstream commands",
    );

    remove_dir(root);
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
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

fn write_inventory_source(root: &Path) {
    for required in [
        "agents/AGENTS.md",
        "agents/command_policy.json",
        "agents/hooks/rules/allowed_commands.json",
        "agents/hooks/rules/forbidden_commands.json",
        "agents/hooks/rules/secret_commit_policy.json",
        "agents/hooks/rules/secret_path_policy.json",
        "claude/settings.base.json",
        "codex/config.toml",
    ] {
        let path = root.join(required);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "fixture\n").unwrap();
    }
    for (name, title) in [("zeta", "Zeta Skill"), ("alpha", "Alpha Skill")] {
        let skill_dir = root.join("agents/skills").join(name);
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: fixture\n---\n\n# {title}\n"),
        )
        .unwrap();
    }
    std::fs::write(
        root.join("agents/skill_rendering.json"),
        r#"{
  "version": 1,
  "skills": {
    "zeta": {
      "claude": {"frontmatter": {"disable-model-invocation": true}},
      "codex": {"openai": {"allow_implicit_invocation": false}}
    }
  }
}
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("agents/hooks.json"),
        r#"{
  "version": 1,
  "claude": {
    "PreToolUse": [{
      "matcher": "^(Bash|Edit)$",
      "hooks": [
        {"command": "$HOME/.claude/hooks/guard.sh"},
        {
          "command": "$HOME/.claude/hooks/lint.sh",
          "if": "Write(*.js)|Edit(*.js)|Write(*.ts)"
        }
      ]
    }]
  },
  "codex": {
    "hooks": {
      "UserPromptSubmit": [{
        "hooks": [{
          "command": "$HOME/.codex/hooks/adapt_shell_command.sh $HOME/.codex/hooks/guard.sh"
        }]
      }]
    }
  }
}
"#,
    )
    .unwrap();
}

fn run_harness<const N: usize>(args: [&str; N]) {
    run_harness_command(Command::new(env!("CARGO_BIN_EXE_agent-harness")).args(args));
}

fn run_harness_stdout<const N: usize>(args: [&str; N]) -> String {
    let output =
        run_harness_command_output(Command::new(env!("CARGO_BIN_EXE_agent-harness")).args(args));
    String::from_utf8(output.stdout).unwrap()
}

fn run_harness_in<const N: usize>(cwd: &Path, args: [&str; N]) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_agent-harness"));
    command.current_dir(cwd).args(args);
    run_harness_command(&mut command);
}

fn run_harness_in_stdout<const N: usize>(cwd: &Path, args: [&str; N]) -> String {
    let mut command = Command::new(env!("CARGO_BIN_EXE_agent-harness"));
    let output = run_harness_command_output(
        command
            .current_dir(cwd)
            .env_remove("AGENT_HARNESS_SOURCE")
            .args(args),
    );
    String::from_utf8(output.stdout).unwrap()
}

fn run_harness_command(command: &mut Command) {
    run_harness_command_output(command);
}

fn run_harness_command_output(command: &mut Command) -> std::process::Output {
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

fn permission_set(settings: &Value, decision: &str) -> BTreeSet<String> {
    settings["permissions"][decision]
        .as_array()
        .into_iter()
        .flatten()
        .map(|value| value.as_str().unwrap().to_owned())
        .collect()
}

fn array_items(value: &Value) -> impl Iterator<Item = &Value> {
    value.as_array().into_iter().flatten()
}

fn policy_decision_name(settings_decision: &str) -> &str {
    match settings_decision {
        "deny" => "forbidden",
        other => other,
    }
}

fn bash_permission_prefix(permission: &str) -> Option<String> {
    permission
        .strip_prefix("Bash(")
        .and_then(|permission| permission.strip_suffix(":*)"))
        .map(str::to_owned)
}

fn frontmatter_key(line: &str) -> Option<String> {
    if line.starts_with(char::is_whitespace) {
        return None;
    }
    line.split_once(':').map(|(key, _)| key.to_owned())
}

fn is_common_frontmatter_key(key: &str) -> bool {
    matches!(key, "name" | "description")
}

fn non_generated_permissions(settings: &Value, decision: &str) -> BTreeSet<String> {
    permission_set(settings, decision)
        .iter()
        .filter(|permission| !permission.starts_with("Bash("))
        .filter(|permission| !permission.starts_with("Edit("))
        .filter(|permission| !permission.starts_with("Write("))
        .cloned()
        .collect()
}

fn assert_all_hook_commands_resolve(value: &Value) {
    let mut commands = Vec::new();
    collect_commands(value, &mut commands);
    let missing = commands
        .iter()
        .filter_map(|command| {
            let resolved = resolve_hook_script(command);
            (!resolved.is_file()).then(|| format!("{command} -> {}", resolved.display()))
        })
        .collect::<Vec<_>>();

    assert!(missing.is_empty(), "missing hook scripts: {missing:#?}");
}

fn collect_commands(value: &Value, commands: &mut Vec<String>) {
    match value {
        Value::Array(items) => {
            for item in items {
                collect_commands(item, commands);
            }
        }
        Value::Object(object) => {
            if let Some(command) = object.get("command").and_then(Value::as_str) {
                commands.push(command.to_owned());
            }
            for item in object.values() {
                collect_commands(item, commands);
            }
        }
        _ => {}
    }
}

fn resolve_hook_script(script: &str) -> PathBuf {
    let root = repo_root();
    let script = hook_script_path(script);
    if let Some(path) = script.strip_prefix("$HOME/.claude/hooks/") {
        return root.join("agents/hooks").join(path);
    }
    if let Some(path) = script.strip_prefix("$HOME/.claude/statusline/") {
        return root.join("claude/statusline").join(path);
    }
    if let Some(path) = script.strip_prefix("$HOME/.codex/hooks/") {
        return root.join("codex/hooks").join(path);
    }
    PathBuf::from(script)
}

fn hook_script_path(command: &str) -> &str {
    let mut words = command.split_whitespace();
    match words.next() {
        Some("bash") => words
            .next()
            .map(|script| script.trim_matches('"'))
            .unwrap_or("bash"),
        Some(script) => script.trim_matches('"'),
        None => command,
    }
}

fn assert_no_duplicate_commands_per_codex_group(hooks: &Value) {
    for event in hooks["hooks"].as_object().unwrap().values() {
        for group in array_items(event) {
            let mut seen = BTreeSet::new();
            for hook in array_items(&group["hooks"]) {
                let command = hook["command"].as_str().unwrap();
                assert!(seen.insert(command), "duplicate hook command: {command}");
            }
        }
    }
}

fn assert_policy_covers_permissions(policy: &Value, settings: &Value, decision: &str) {
    let policy_patterns = policy_patterns(policy, decision);
    let missing = permission_set(settings, decision)
        .into_iter()
        .filter_map(|permission| bash_permission_prefix(&permission))
        .filter(|prefix| !policy_patterns.contains(prefix))
        .collect::<Vec<_>>();

    assert!(missing.is_empty(), "missing policy patterns: {missing:#?}");
}

fn policy_patterns(policy: &Value, decision: &str) -> BTreeSet<String> {
    let policy_decision = policy_decision_name(decision);
    policy["rules"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|rule| rule["decision"].as_str() == Some(policy_decision))
        .map(|rule| {
            rule["pattern"]
                .as_array()
                .unwrap()
                .iter()
                .map(|part| part.as_str().unwrap())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect()
}

fn assert_source_skill_frontmatter_is_common() {
    let skills = repo_root().join("agents/skills");
    for entry in std::fs::read_dir(skills).unwrap() {
        let skill = entry.unwrap().path();
        let skill_file = skill.join("SKILL.md");
        if !skill_file.is_file() {
            continue;
        }
        let content = std::fs::read_to_string(&skill_file).unwrap();
        let keys = frontmatter_keys(&content);
        let invalid = keys
            .iter()
            .filter(|key| !is_common_frontmatter_key(key))
            .collect::<Vec<_>>();
        assert!(
            invalid.is_empty(),
            "{} has provider-specific frontmatter keys: {invalid:#?}",
            skill_file.display(),
        );
    }
}

fn frontmatter_keys(content: &str) -> BTreeSet<String> {
    let frontmatter = content
        .strip_prefix("---\n")
        .and_then(|body| body.split_once("\n---\n"))
        .map(|(frontmatter, _body)| frontmatter)
        .unwrap();
    frontmatter.lines().filter_map(frontmatter_key).collect()
}

fn assert_contains(path: &Path, expected: &str) {
    let content = std::fs::read_to_string(path).unwrap();
    assert!(
        content.contains(expected),
        "{} did not contain {expected:?}",
        path.display(),
    );
}
