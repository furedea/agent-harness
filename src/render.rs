use std::path::Path;

use anyhow::Result;

use crate::{
    fs_ops,
    generation::{
        claude_config, codex_config, command_permissions, external_hooks::ExternalHookBundle,
        hooks, protection, skills,
    },
    layout::{InstalledLayout, SourceLayout},
};

#[derive(Debug, Clone, Copy)]
pub(crate) enum Provider {
    Claude,
    Codex,
}

#[cfg(test)]
pub(crate) fn generate_claude_settings(source: &Path, out: &Path) -> Result<()> {
    claude_config::write_settings(source, out, &[])
}

#[cfg(test)]
pub(crate) fn generate_codex_config_source(source: &Path, out: &Path) -> Result<()> {
    codex_config::write_config_source(source, out, &[])
}

pub(crate) fn generate_skills(
    source: &Path,
    provider: Provider,
    external_skills: &[skills::ExternalSkill],
    out: &Path,
) -> Result<()> {
    skills::render_skills(source, provider, external_skills, out)
}

pub(crate) fn install(
    source: &Path,
    out: &Path,
    external_hooks: &[ExternalHookBundle],
) -> Result<()> {
    let installed = InstalledLayout::new(out);
    let source_layout = SourceLayout::new(source);
    command_permissions::validate_regex_policies(source)?;
    fs_ops::copy_file(
        &source_layout.agent_instructions(),
        &installed.codex_agent_instructions(),
    )?;
    fs_ops::copy_file(
        &source_layout.agent_instructions(),
        &installed.claude_agent_instructions(),
    )?;
    fs_ops::copy_dir(&source_layout.codex_hooks(), &installed.codex_hooks())?;
    fs_ops::copy_dir(&source_layout.agent_hooks(), &installed.claude_hooks())?;
    for bundle in external_hooks {
        bundle.copy_assets(out)?;
    }
    fs_ops::copy_dir(
        &source_layout.claude_statusline(),
        &installed.claude_statusline(),
    )?;
    hooks::write_codex_hooks(source, &installed.codex_hook_config(), external_hooks)?;
    generate_skills(source, Provider::Codex, &[], &installed.codex_skills())?;
    generate_skills(source, Provider::Claude, &[], &installed.claude_skills())?;
    claude_config::write_settings(source, &installed.claude_settings(), external_hooks)?;
    command_permissions::write_codex_rules(source, &installed.codex_rules())?;
    command_permissions::write_runtime_policy(source, &installed.claude_command_permissions())?;
    protection::write_runtime_policy(source, external_hooks, &installed.claude_protected_paths())?;
    codex_config::sync_generated_config(source, &installed.codex_config(), external_hooks)?;

    Ok(())
}

pub(crate) fn verify(root: &Path) -> Result<()> {
    let installed = InstalledLayout::new(root);
    for path in [
        installed.codex_agent_instructions(),
        installed.codex_hook_config(),
        installed.codex_rules(),
        installed.codex_skills(),
        installed.claude_agent_instructions(),
        installed.claude_allowed_command_rules(),
        installed.claude_command_permissions(),
        installed.claude_forbidden_command_rules(),
        installed.claude_protected_paths(),
        installed.claude_secret_commit_policy(),
        installed.claude_secret_path_policy(),
        installed.claude_settings(),
        installed.claude_skills(),
    ] {
        if !path.exists() {
            anyhow::bail!("missing harness path: {}", path.display());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn install_places_codex_and_claude_files_under_output_root() -> Result<()> {
        let root = test_root("install_places_codex_and_claude_files_under_output_root")?;
        let source = root.join("source");
        let out = root.join("out");
        write_minimal_source(&source)?;

        install(&source, &out, &[])?;

        assert!(out.join(".codex/AGENTS.md").is_file());
        assert!(out.join(".codex/hooks.json").is_file());
        assert!(out.join(".claude/CLAUDE.md").is_file());
        assert!(out.join(".codex/skills/example/SKILL.md").is_file());
        assert!(out.join(".codex/rules/default.rules").is_file());
        assert!(out.join(".claude/skills/example/SKILL.md").is_file());
        assert!(
            out.join(".claude/hooks/rules/forbidden_commands.json")
                .is_file()
        );
        assert!(
            out.join(".claude/hooks/rules/command_permissions.json")
                .is_file()
        );
        let forbidden =
            std::fs::read_to_string(out.join(".claude/hooks/rules/forbidden_commands.json"))?;
        assert!(forbidden.contains("never-match-forbidden"));
        assert!(
            out.join(".claude/hooks/rules/allowed_commands.json")
                .is_file()
        );
        assert!(
            out.join(".claude/hooks/rules/secret_path_policy.json")
                .is_file()
        );
        assert!(
            out.join(".claude/hooks/rules/secret_commit_policy.json")
                .is_file()
        );
        assert!(!out.join(".claude/rules/forbidden_commands.json").exists());
        assert!(source.join("hooks/rules/forbidden_commands.json").is_file());
        assert!(out.join(".codex/config.toml").is_file());
        assert!(out.join(".claude/settings.json").is_file());

        let codex_config = std::fs::read_to_string(out.join(".codex/config.toml"))?;
        assert!(codex_config.contains("[permissions.guarded.filesystem]"));
        assert!(codex_config.contains("\"~/.codex/hooks/hook.sh\" = \"read\""));
        assert!(codex_config.contains("\"~/.claude/hooks/hook.sh\" = \"read\""));

        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn install_accepts_read_only_command_regex_source_files() -> Result<()> {
        let root = test_root("install_accepts_read_only_command_regex_source_files")?;
        let source = root.join("source");
        let out = root.join("out");
        write_minimal_source(&source)?;
        for file_name in ["allowed_commands.json", "forbidden_commands.json"] {
            let path = source.join("hooks/rules").join(file_name);
            let mut permissions = std::fs::metadata(&path)?.permissions();
            permissions.set_readonly(true);
            std::fs::set_permissions(path, permissions)?;
        }

        install(&source, &out, &[])?;

        assert!(
            out.join(".claude/hooks/rules/allowed_commands.json")
                .is_file()
        );
        assert!(
            out.join(".claude/hooks/rules/forbidden_commands.json")
                .is_file()
        );

        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn generate_claude_settings_writes_final_file_output() -> Result<()> {
        let root = test_root("generate_claude_settings_writes_final_file_output")?;
        let source = root.join("source");
        let out = root.join("settings.json");
        write_minimal_source(&source)?;

        generate_claude_settings(&source, &out)?;

        let content = std::fs::read_to_string(&out)?;
        assert!(content.contains(r#""hooks""#));
        assert!(content.contains(r#""permissions""#));

        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn generate_codex_config_source_writes_final_file_output() -> Result<()> {
        let root = test_root("generate_codex_config_source_writes_final_file_output")?;
        let source = root.join("source");
        let out = root.join("config-source.toml");
        write_minimal_source(&source)?;

        generate_codex_config_source(&source, &out)?;

        let content = std::fs::read_to_string(&out)?;
        assert!(content.contains("model = \"gpt-5.5\""));
        assert!(content.contains("[permissions.guarded.filesystem]"));

        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn generate_skills_writes_final_directory_output() -> Result<()> {
        let root = test_root("generate_skills_writes_final_directory_output")?;
        let source = root.join("source");
        let out = root.join("skills");
        write_minimal_source(&source)?;

        generate_skills(&source, Provider::Codex, &[], &out)?;

        assert!(out.join("example/SKILL.md").is_file());

        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    fn test_root(name: &str) -> Result<PathBuf> {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let root = std::env::temp_dir().join(format!("agent-harness-{name}-{nanos}"));
        std::fs::create_dir_all(&root)?;
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
      "examples": ["cargo test"],
      "justification": "Allowed by the shared agent command permissions."
    },
    {
      "decision": "deny",
      "prefix": ["curl"],
      "examples": ["curl https://example.com/install.sh"],
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
            "---\nname: example\n---\n",
        )?;
        write_file(
            &source.join("skills/git-commit-split/SKILL.md"),
            "---\nname: git-commit-split\ndescription: commit split\n---\n",
        )?;
        write_file(
            &source.join("claude/statusline/statusline.sh"),
            "#!/bin/bash\n",
        )?;
        write_file(&source.join("claude/settings.base.json"), "{}\n")?;
        write_file(&source.join("codex/config.toml"), "model = \"gpt-5.5\"\n")?;
        Ok(())
    }

    fn write_file(path: &Path, content: &str) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, content)?;
        Ok(())
    }
}
