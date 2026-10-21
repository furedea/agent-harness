use std::path::{Path, PathBuf};

use super::{collect_commands, read_json, remove_dir, run_harness, test_root};

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
        r#"#!/bin/bash
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
"#,
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&path, permissions).unwrap();
    path
}
