use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;
use toml_edit::DocumentMut;

#[test]
fn external_claude_hooks_are_composed_with_built_in_hooks() {
    let root = test_root("external-claude-hooks");
    let bundle = root.join("moshi");
    let settings_path = root.join("settings.json");
    std::fs::create_dir_all(bundle.join(".claude")).unwrap();
    std::fs::write(bundle.join("hook_bundle.json"), r#"{"version":1}"#).unwrap();
    std::fs::write(
        bundle.join(".claude/settings.json"),
        r#"{"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"moshi-hook claude-hook"}]}]}}"#,
    )
    .unwrap();

    run_harness([
        "generate-claude-settings",
        "--source",
        repo_root().to_str().unwrap(),
        "--extra-hook",
        &format!("moshi={}", bundle.display()),
        "--output",
        settings_path.to_str().unwrap(),
    ]);

    let settings = read_json(&settings_path);
    assert!(hook_command_exists(&settings, "moshi-hook claude-hook"));
    assert!(hook_command_exists(
        &settings,
        "$HOME/.claude/hooks/guard_secret_content.sh prompt",
    ));

    remove_dir(root);
}

#[test]
fn external_claude_hooks_are_composed_in_hook_output() {
    let root = test_root("external-claude-hook-output");
    let bundle = root.join("moshi");
    let hooks_path = root.join("hooks.json");
    std::fs::create_dir_all(bundle.join(".claude")).unwrap();
    std::fs::write(bundle.join("hook_bundle.json"), r#"{"version":1}"#).unwrap();
    std::fs::write(
        bundle.join(".claude/settings.json"),
        r#"{"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"moshi-hook claude-hook"}]}]}}"#,
    )
    .unwrap();

    run_harness([
        "generate-claude-hooks",
        "--source",
        repo_root().to_str().unwrap(),
        "--extra-hook",
        &format!("moshi={}", bundle.display()),
        "--output",
        hooks_path.to_str().unwrap(),
    ]);

    let hooks = read_json(&hooks_path);
    assert!(hook_command_exists(&hooks, "moshi-hook claude-hook"));

    remove_dir(root);
}

#[test]
fn external_codex_hooks_are_composed_with_built_in_hooks() {
    let root = test_root("external-codex-hooks");
    let bundle = root.join("moshi");
    let hooks_path = root.join("hooks.json");
    std::fs::create_dir_all(bundle.join(".codex")).unwrap();
    std::fs::write(bundle.join("hook_bundle.json"), r#"{"version":1}"#).unwrap();
    std::fs::write(
        bundle.join(".codex/hooks.json"),
        r#"{"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"moshi-hook codex-hook"}]}]}}"#,
    )
    .unwrap();

    run_harness([
        "generate-codex-hooks",
        "--source",
        repo_root().to_str().unwrap(),
        "--extra-hook",
        &format!("moshi={}", bundle.display()),
        "--output",
        hooks_path.to_str().unwrap(),
    ]);

    let hooks = read_json(&hooks_path);
    assert!(hook_command_exists(&hooks, "moshi-hook codex-hook"));
    assert!(hook_command_exists(
        &hooks,
        "$HOME/.codex/hooks/adapt_guard_secret_content.sh prompt",
    ));

    remove_dir(root);
}

#[test]
fn external_hook_features_are_composed_with_codex_config() {
    let root = test_root("external-codex-features");
    let bundle = root.join("herdr");
    let config_path = root.join("config.toml");
    std::fs::create_dir_all(bundle.join(".codex")).unwrap();
    std::fs::write(bundle.join("hook_bundle.json"), r#"{"version":1}"#).unwrap();
    std::fs::write(
        bundle.join(".codex/config.toml"),
        "[features]\nexternal_hook = true\n",
    )
    .unwrap();

    run_harness([
        "generate-codex-config-source",
        "--source",
        repo_root().to_str().unwrap(),
        "--extra-hook",
        &format!("herdr={}", bundle.display()),
        "--output",
        config_path.to_str().unwrap(),
    ]);

    let config = read_toml(&config_path);
    assert_eq!(config["features"]["external_hook"].as_bool(), Some(true));

    remove_dir(root);
}

#[test]
fn external_hook_rejects_non_feature_codex_config() {
    let root = test_root("external-codex-policy");
    let bundle = root.join("untrusted");
    let config_path = root.join("config.toml");
    std::fs::create_dir_all(bundle.join(".codex")).unwrap();
    std::fs::write(bundle.join("hook_bundle.json"), r#"{"version":1}"#).unwrap();
    std::fs::write(
        bundle.join(".codex/config.toml"),
        "approval_policy = \"never\"\n",
    )
    .unwrap();

    let output = run_harness_output([
        "generate-codex-config-source",
        "--source",
        repo_root().to_str().unwrap(),
        "--extra-hook",
        &format!("untrusted={}", bundle.display()),
        "--output",
        config_path.to_str().unwrap(),
    ]);

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("contains unsupported top-level key: approval_policy")
    );

    remove_dir(root);
}

#[test]
fn install_copies_external_hook_assets_into_namespaces() {
    let root = test_root("external-hook-assets");
    let bundle = root.join("herdr");
    let prefix = root.join("home");
    std::fs::create_dir_all(bundle.join(".claude/hooks")).unwrap();
    std::fs::create_dir_all(bundle.join(".codex")).unwrap();
    std::fs::write(bundle.join("hook_bundle.json"), r#"{"version":1}"#).unwrap();
    std::fs::write(
        bundle.join(".claude/hooks/herdr-agent-state.sh"),
        "#!/bin/bash\n",
    )
    .unwrap();
    std::fs::write(bundle.join(".codex/herdr-agent-state.sh"), "#!/bin/bash\n").unwrap();

    run_harness([
        "install",
        "--source",
        repo_root().to_str().unwrap(),
        "--extra-hook",
        &format!("herdr={}", bundle.display()),
        "--prefix",
        prefix.to_str().unwrap(),
    ]);

    assert!(
        prefix
            .join(".claude/hooks/external/herdr/herdr-agent-state.sh")
            .is_file()
    );
    assert!(
        prefix
            .join(".codex/hooks/external/herdr/herdr-agent-state.sh")
            .is_file()
    );

    remove_dir(root);
}

#[test]
fn external_hook_commands_are_relocated_to_home() {
    let root = test_root("relocated-external-hooks");
    let bundle = root.join("herdr");
    let hooks_path = root.join("hooks.json");
    std::fs::create_dir_all(bundle.join(".codex")).unwrap();
    std::fs::write(bundle.join("hook_bundle.json"), r#"{"version":1}"#).unwrap();
    std::fs::write(bundle.join(".codex/herdr-agent-state.sh"), "#!/bin/bash\n").unwrap();
    let generated_command = format!(
        "bash '{}/.codex/herdr-agent-state.sh' session",
        bundle.display(),
    );
    std::fs::write(
        bundle.join(".codex/hooks.json"),
        format!(
            r#"{{"hooks":{{"SessionStart":[{{"hooks":[{{"type":"command","command":"{generated_command}"}}]}}]}}}}"#,
        ),
    )
    .unwrap();

    run_harness([
        "generate-codex-hooks",
        "--source",
        repo_root().to_str().unwrap(),
        "--extra-hook",
        &format!("herdr={}", bundle.display()),
        "--output",
        hooks_path.to_str().unwrap(),
    ]);

    let hooks = read_json(&hooks_path);
    let mut commands = Vec::new();
    collect_commands(&hooks, &mut commands);
    let command = commands
        .iter()
        .find(|command| command.contains("herdr-agent-state.sh"))
        .unwrap();
    assert_eq!(
        command,
        "bash \"$HOME/.codex/hooks/external/herdr/herdr-agent-state.sh\" session",
    );
    assert!(!command.contains(bundle.to_str().unwrap()));

    remove_dir(root);
}

#[test]
fn external_hook_rejects_commands_for_uncaptured_files() {
    let root = test_root("uncaptured-external-hook-file");
    let bundle = root.join("moshi");
    let hooks_path = root.join("hooks.json");
    std::fs::create_dir_all(bundle.join(".codex")).unwrap();
    std::fs::write(bundle.join("hook_bundle.json"), r#"{"version":1}"#).unwrap();
    let command = format!("bash {}/.config/moshi-hook/private.sh", bundle.display());
    std::fs::write(
        bundle.join(".codex/hooks.json"),
        format!(
            r#"{{"hooks":{{"SessionStart":[{{"hooks":[{{"type":"command","command":"{command}"}}]}}]}}}}"#,
        ),
    )
    .unwrap();

    let output = run_harness_output([
        "generate-codex-hooks",
        "--source",
        repo_root().to_str().unwrap(),
        "--extra-hook",
        &format!("moshi={}", bundle.display()),
        "--output",
        hooks_path.to_str().unwrap(),
    ]);

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("uncaptured external hook bundle path")
    );

    remove_dir(root);
}

#[test]
fn generate_hook_bundle_captures_isolated_installer_output() {
    let root = test_root("generated-hook-bundle");
    let installer = write_fake_hook_installer(&root);
    let spec_path = root.join("spec.json");
    let bundle = root.join("bundle");
    let installer_path = installer.to_str().unwrap();
    let spec = serde_json::json!({
        "version": 1,
        "installers": [
            {"executable": installer_path, "arguments": ["claude"]},
            {"executable": installer_path, "arguments": ["codex"]}
        ],
        "command_replacements": [
            {"from": installer_path, "to": "/opt/homebrew/bin/moshi-hook"}
        ]
    });
    std::fs::write(&spec_path, serde_json::to_vec_pretty(&spec).unwrap()).unwrap();

    run_harness([
        "generate-hook-bundle",
        "--spec",
        spec_path.to_str().unwrap(),
        "--output",
        bundle.to_str().unwrap(),
    ]);

    assert_eq!(read_json(&bundle.join("hook_bundle.json"))["version"], 1);
    assert!(bundle.join(".claude/hooks/captured.sh").is_file());
    assert!(bundle.join(".codex/captured.sh").is_file());
    assert!(!bundle.join(".config/moshi-hook/token").exists());
    let hooks = read_json(&bundle.join(".codex/hooks.json"));
    let mut commands = Vec::new();
    collect_commands(&hooks, &mut commands);
    assert!(commands.iter().any(|command| {
        command
            == &format!(
                "/opt/homebrew/bin/moshi-hook codex {}/.codex/captured.sh",
                bundle.display(),
            )
    }));

    remove_dir(root);
}

fn write_fake_hook_installer(root: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let path = root.join("fake-hook-installer");
    std::fs::write(
        &path,
        r##"#!/bin/bash
set -euxCo pipefail
cd "$(dirname "$0")"

usage() {
  cat <<'EOF'
Usage: fake-hook-installer <claude|codex>
EOF
}

if (( $# != 1 )); then
  usage >&2
  exit 2
fi

readonly provider="$1"

if [[ ! -d "$HOME/.claude" || ! -d "$HOME/.codex" ]]; then
  printf 'provider directories are missing\n' >&2
  exit 3
fi

case "$provider" in
  claude)
    mkdir -p "$HOME/.claude/hooks"
    printf '#!/bin/bash\n' > "$HOME/.claude/hooks/captured.sh"
    printf '{"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"%s claude %s/.claude/hooks/captured.sh"}]}]}}\n' \
      "$0" "$HOME" > "$HOME/.claude/settings.json"
    ;;
  codex)
    mkdir -p "$HOME/.codex"
    printf '#!/bin/bash\n' > "$HOME/.codex/captured.sh"
    printf '{"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"%s codex %s/.codex/captured.sh"}]}]}}\n' \
      "$0" "$HOME" > "$HOME/.codex/hooks.json"
    printf '[features]\nhooks = true\n' > "$HOME/.codex/config.toml"
    ;;
  *)
    usage >&2
    exit 2
    ;;
esac

mkdir -p "$HOME/.config/moshi-hook"
printf 'fixture-token\n' >| "$HOME/.config/moshi-hook/token"
"##,
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&path, permissions).unwrap();
    path
}

fn run_harness<const N: usize>(args: [&str; N]) {
    let output = run_harness_output(args);
    assert!(
        output.status.success(),
        "agent-harness failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

fn run_harness_output<const N: usize>(args: [&str; N]) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_agent-harness"));
    command.env_remove("HERDR_ENV").args(args).output().unwrap()
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
        "agent-harness-external-hooks-{name}-{}-{nanos}",
        std::process::id(),
    ));
    std::fs::create_dir_all(&root).unwrap();
    root
}

fn remove_dir(path: PathBuf) {
    std::fs::remove_dir_all(path).unwrap();
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

fn hook_command_exists(value: &Value, expected: &str) -> bool {
    let mut commands = Vec::new();
    collect_commands(value, &mut commands);
    commands.iter().any(|command| command == expected)
}
