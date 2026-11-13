use crate::provider::spec::ProviderId;
use crate::shell::fixtures::regular_files;
use crate::shell::sync::{SyncPaths, sync};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

#[test]
fn install_places_codex_and_claude_files_under_output_root() -> Result<()> {
    let root = test_root("install_places_codex_and_claude_files_under_output_root")?;
    let source = root.join("source");
    let out = root.join("out");
    write_minimal_source(&source)?;

    install_home(&source, &out)?;

    let missing_files: Vec<&str> = [
        ".codex/AGENTS.md",
        ".codex/hooks.json",
        ".claude/CLAUDE.md",
        ".codex/skills/example/SKILL.md",
        ".codex/rules/default.rules",
        ".claude/skills/example/SKILL.md",
        ".claude/hooks/rules/forbidden_commands.json",
        ".claude/hooks/rules/command_permissions.json",
        ".claude/hooks/rules/allowed_commands.json",
        ".claude/hooks/rules/secret_path_policy.json",
        ".claude/hooks/rules/secret_commit_policy.json",
        ".codex/config.toml",
        ".claude/settings.json",
        ".config/devin/config.json",
    ]
    .into_iter()
    .filter(|relative| !crate::shell::fixtures::is_file(out.join(relative)))
    .collect();
    assert!(missing_files.is_empty(), "missing files: {missing_files:?}");
    let forbidden = crate::shell::fixtures::read_to_string(
        out.join(".claude/hooks/rules/forbidden_commands.json"),
    )?;
    assert!(forbidden.contains("never-match-forbidden"));
    assert!(!crate::shell::fixtures::exists(
        out.join(".claude/rules/forbidden_commands.json")
    ));
    assert!(crate::shell::fixtures::is_file(
        source.join("hooks/rules/forbidden_commands.json")
    ));
    assert!(crate::shell::fixtures::is_dir(out.join(".devin/hooks")));

    let codex_config = crate::shell::fixtures::read_to_string(out.join(".codex/config.toml"))?;
    assert!(codex_config.contains("[permissions.guarded.filesystem]"));
    assert!(codex_config.contains("\"~/.codex/hooks/hook.sh\" = \"read\""));
    assert!(codex_config.contains("\"~/.claude/hooks/hook.sh\" = \"read\""));

    crate::shell::fixtures::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn install_accepts_read_only_command_regex_source_files() -> Result<()> {
    let root = test_root("install_accepts_read_only_command_regex_source_files")?;
    let source = root.join("source");
    let out = root.join("out");
    write_minimal_source(&source)?;
    ["allowed_commands.json", "forbidden_commands.json"]
        .into_iter()
        .try_for_each(|file_name| set_readonly(&source.join("hooks/rules").join(file_name)))?;

    install_home(&source, &out)?;

    assert!(crate::shell::fixtures::is_file(
        out.join(".claude/hooks/rules/allowed_commands.json")
    ));
    assert!(crate::shell::fixtures::is_file(
        out.join(".claude/hooks/rules/forbidden_commands.json")
    ));

    crate::shell::fixtures::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn verify_rejects_a_missing_codex_config() -> Result<()> {
    let root = test_root("verify-missing-codex-config")?;
    let source = root.join("source");
    let out = root.join("out");
    write_minimal_source(&source)?;
    install_home(&source, &out)?;
    verify(&out)?;
    crate::shell::fixtures::remove_file(out.join(".codex/config.toml"))?;

    let error = verify(&out).unwrap_err();

    assert!(error.to_string().contains(".codex/config.toml"));
    crate::shell::fixtures::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn install_preserves_existing_files_when_source_generation_fails() -> Result<()> {
    let root = test_root("install-invalid-source")?;
    let source = root.join("source");
    let out = root.join("out");
    write_minimal_source(&source)?;
    install_home(&source, &out)?;
    let before = installed_contents(&out)?;
    write_file(&source.join("AGENTS.md"), "replacement instructions\n")?;
    crate::shell::fixtures::remove_file(source.join("hooks/hook.sh"))?;
    write_file(&source.join("codex/config.toml"), "invalid TOML [")?;

    let error = install_home(&source, &out).unwrap_err();

    assert!(format!("{error:#}").contains("TOML"));
    assert_eq!(installed_contents(&out)?, before);
    crate::shell::fixtures::remove_dir_all(root)?;
    Ok(())
}

fn install_home(source: &Path, out: &Path) -> Result<()> {
    let runtime_root = RuntimeRoot::home();
    let request = RenderRequest {
        dsh_profile: "web",
        source,
        external_hooks: &[],
        runtime_root: &runtime_root,
    };
    install(&request, out)
}

fn installed_contents(root: &Path) -> Result<Vec<(PathBuf, Vec<u8>)>> {
    regular_files(root)?
        .into_iter()
        .map(|path| {
            Ok((
                path.strip_prefix(root)?.to_path_buf(),
                crate::shell::fixtures::read(path)?,
            ))
        })
        .collect()
}

#[test]
fn verify_rejects_required_paths_with_the_wrong_file_type() -> Result<()> {
    let root = test_root("verify-wrong-file-type")?;
    let source = root.join("source");
    let out = root.join("out");
    write_minimal_source(&source)?;
    install_home(&source, &out)?;
    let replace_with_directory = |path: &Path| crate::shell::fixtures::create_dir(path);
    let replace_with_file = |path: &Path| crate::shell::fixtures::write(path, "not a directory");
    assert_verify_rejects(&out, ".claude/settings.json", replace_with_directory)?;
    assert_verify_rejects(&out, ".codex/skills", replace_with_file)?;
    verify(&out)?;
    crate::shell::fixtures::remove_dir_all(root)?;
    Ok(())
}

fn assert_verify_rejects(
    out: &Path,
    relative: &str,
    replace: impl Fn(&Path) -> std::io::Result<()>,
) -> Result<()> {
    let path = out.join(relative);
    let backup = path.with_extension("backup");
    crate::shell::fixtures::rename(&path, &backup)?;
    replace(&path)?;

    let error = verify(out).unwrap_err();

    assert!(error.to_string().contains(relative));
    crate::shell::fixtures::remove_dir(&path)
        .or_else(|_| crate::shell::fixtures::remove_file(&path))?;
    crate::shell::fixtures::rename(backup, path)?;
    Ok(())
}

#[test]
fn generate_claude_settings_writes_final_file_output() -> Result<()> {
    let root = test_root("generate_claude_settings_writes_final_file_output")?;
    let source = root.join("source");
    let out = root.join("settings.json");
    write_minimal_source(&source)?;

    generate_claude_settings(&source, &out)?;

    let content = crate::shell::fixtures::read_to_string(&out)?;
    assert!(content.contains(r#""hooks""#));
    assert!(content.contains(r#""permissions""#));

    crate::shell::fixtures::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn generate_codex_config_source_writes_final_file_output() -> Result<()> {
    let root = test_root("generate_codex_config_source_writes_final_file_output")?;
    let source = root.join("source");
    let out = root.join("config-source.toml");
    write_minimal_source(&source)?;

    generate_codex_config_source(&source, &out)?;

    let content = crate::shell::fixtures::read_to_string(&out)?;
    assert!(content.contains("model = \"gpt-5.5\""));
    assert!(content.contains("[permissions.guarded.filesystem]"));

    crate::shell::fixtures::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn generate_skills_writes_final_directory_output() -> Result<()> {
    let root = test_root("generate_skills_writes_final_directory_output")?;
    let source = root.join("source");
    let out = root.join("skills");
    write_minimal_source(&source)?;

    generate_skills(
        &source,
        &[],
        skills::SkillTarget {
            provider: Provider::Codex,
            path: &out,
        },
    )?;

    assert!(crate::shell::fixtures::is_file(
        out.join("example/SKILL.md")
    ));

    crate::shell::fixtures::remove_dir_all(root)?;
    Ok(())
}

fn test_root(name: &str) -> Result<PathBuf> {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let root = std::env::temp_dir().join(format!("agent-harness-{name}-{nanos}"));
    crate::shell::fixtures::create_dir_all(&root)?;
    Ok(root)
}

fn write_minimal_source(source: &Path) -> Result<()> {
    write_file(
        &source.join("manifest.json"),
        r#"{"version":1,"runtime_commands":[]}"#,
    )?;
    write_file(&source.join("AGENTS.md"), "agent instructions\n")?;
    write_file(
        &source.join("command_permissions.json"),
        r#"{
  "version": 1,
  "rules": [
    {
      "decision": "allow",
      "prefix": ["cargo"],
      "justification": "Allowed by the shared agent command permissions."
    },
    {
      "decision": "deny",
      "prefix": ["curl"],
      "justification": "Do not fetch remote scripts or content from Codex."
    }
  ]
}
"#,
    )?;
    write_file(&source.join("hooks/hook.sh"), "#!/bin/bash\n")?;
    write_file(
        &source.join("hooks/rules/secret_commit_policy.json"),
        r#"{"version":1,"rules":[{"pattern":"never-match","reason":"test"}]}"#,
    )?;
    write_file(
        &source.join("hooks/rules/secret_path_policy.json"),
        r#"{
  "version": 1,
  "rules": [
    {
      "pattern": ".env*",
      "access": ["read"],
      "reason": "Environment files may contain credentials."
    }
  ]
}
"#,
    )?;
    write_file(
        &source.join("hooks/rules/allowed_commands.json"),
        r#"{"version":1,"rules":[{"patterns":["^cargo test$"],"justification":"test"}]}"#,
    )?;
    write_file(
        &source.join("hooks/rules/forbidden_commands.json"),
        r#"{"version":1,"rules":[{"patterns":["^never-match-forbidden$"],"justification":"test"}]}"#,
    )?;
    write_file(
        &source.join("hooks.json"),
        r#"{
  "version": 1,
  "claude": {},
  "codex": {
    "hooks": {}
  }
}
"#,
    )?;
    write_file(&source.join("codex/hooks/hook.sh"), "#!/bin/bash\n")?;
    write_file(
        &source.join("skills/example/SKILL.md"),
        "---\nname: example\ndescription: example skill\n---\n\n# Example\n",
    )?;
    write_file(
        &source.join("skills/git-commit-split/SKILL.md"),
        "---\nname: git-commit-split\ndescription: commit split\n---\n\n# Commit split\n",
    )?;
    write_file(
        &source.join("claude/statusline/statusline.sh"),
        "#!/bin/bash\n",
    )?;
    write_file(&source.join("claude/settings.json"), "{}\n")?;
    write_file(&source.join("codex/config.toml"), "model = \"gpt-5.5\"\n")?;
    Ok(())
}

fn set_readonly(path: &Path) -> Result<()> {
    let mut permissions = crate::shell::fixtures::metadata(path)?.permissions();
    permissions.set_readonly(true);
    crate::shell::fixtures::set_permissions(path, permissions)?;
    Ok(())
}

fn write_file(path: &Path, content: &str) -> Result<()> {
    path.parent()
        .map(crate::shell::fixtures::create_dir_all)
        .transpose()?;
    crate::shell::fixtures::write(path, content)?;
    Ok(())
}

#[test]
fn install_keeps_top_level_claude_settings_it_does_not_generate() -> Result<()> {
    let root = test_root("install_keeps_top_level_claude_settings_it_does_not_generate")?;
    let source = root.join("source");
    let out = root.join("out");
    write_minimal_source(&source)?;
    crate::shell::fixtures::create_dir_all(out.join(".claude"))?;
    crate::shell::fixtures::write(
        out.join(".claude/settings.json"),
        r#"{"model":"user-model","hooks":{"Stale":[]}}"#,
    )?;

    install_home(&source, &out)?;

    let settings: serde_json::Value = serde_json::from_str(
        &crate::shell::fixtures::read_to_string(out.join(".claude/settings.json"))?,
    )?;
    assert_eq!(settings["model"], "user-model");
    assert!(settings["hooks"].get("Stale").is_none());
    assert!(settings["permissions"].is_object());
    crate::shell::fixtures::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn install_matches_the_merge_the_home_manager_build_applies() -> Result<()> {
    let root = test_root("install_matches_the_merge_the_home_manager_build_applies")?;
    let source = root.join("source");
    let out = root.join("out");
    let generated = root.join("generated.json");
    let synced = root.join("synced/settings.json");
    write_minimal_source(&source)?;
    for target in [&out.join(".claude/settings.json"), &synced] {
        crate::shell::fixtures::create_dir_all(target.parent().unwrap())?;
        crate::shell::fixtures::write(target, r#"{"model":"user-model"}"#)?;
    }
    generate_claude_settings(&source, &generated)?;

    let paths = SyncPaths {
        source: &generated,
        target: &synced,
    };
    sync(ProviderId::Claude.spec(), &paths, &root)?;
    install_home(&source, &out)?;

    assert_eq!(
        crate::shell::fixtures::read_to_string(out.join(".claude/settings.json"))?,
        crate::shell::fixtures::read_to_string(&synced)?
    );
    crate::shell::fixtures::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn install_rejects_a_source_without_hooks_and_keeps_the_installed_hooks() -> Result<()> {
    let root = test_root("install_rejects_a_source_without_hooks_and_keeps_the_installed_hooks")?;
    let source = root.join("source");
    let out = root.join("out");
    write_minimal_source(&source)?;
    crate::shell::fixtures::create_dir_all(out.join(".claude/hooks"))?;
    crate::shell::fixtures::write(out.join(".claude/hooks/installed.sh"), "installed\n")?;
    crate::shell::fixtures::remove_dir_all(source.join("hooks"))?;

    assert!(install_home(&source, &out).is_err());

    assert_eq!(
        crate::shell::fixtures::read_to_string(out.join(".claude/hooks/installed.sh"))?,
        "installed\n"
    );
    crate::shell::fixtures::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn install_leaves_home_paths_to_the_sync_when_the_prefix_is_not_the_home() -> Result<()> {
    let root = test_root("install_leaves_home_paths_to_the_sync")?;
    let source = root.join("source");
    let out = root.join("out");
    write_minimal_source(&source)?;
    crate::shell::fixtures::write(
        source.join("claude/settings.json"),
        r#"{"sandbox":{"excludedCommands":["~/.local/bin/tool *"]}}"#,
    )?;

    install_home(&source, &out)?;

    let settings = crate::shell::fixtures::read_to_string(out.join(".claude/settings.json"))?;
    assert!(settings.contains("\"~/.local/bin/tool *\""));
    crate::shell::fixtures::remove_dir_all(root)?;
    Ok(())
}
