use std::path::Path;

use anyhow::Result;

use crate::{claude_config, codex_config, command_policy, fs_ops, hooks, skills};

#[derive(Debug, Clone, Copy)]
pub enum Provider {
    Claude,
    Codex,
}

pub fn generate_claude_settings(source: &Path, out: &Path) -> Result<()> {
    claude_config::write_settings(source, out)
}

pub fn generate_codex_config_source(source: &Path, out: &Path) -> Result<()> {
    codex_config::write_config_source(source, out)
}

pub fn generate_skills(source: &Path, provider: Provider, out: &Path) -> Result<()> {
    skills::render_skills(source, provider, out)
}

pub fn install(source: &Path, out: &Path) -> Result<()> {
    fs_ops::copy_file(
        &source.join("agents/AGENTS.md"),
        &out.join(".codex/AGENTS.md"),
    )?;
    fs_ops::copy_file(
        &source.join("agents/AGENTS.md"),
        &out.join(".claude/CLAUDE.md"),
    )?;
    fs_ops::copy_dir(&source.join("codex/hooks"), &out.join(".codex/hooks"))?;
    fs_ops::copy_dir(&source.join("agents/hooks"), &out.join(".claude/hooks"))?;
    fs_ops::copy_dir(
        &source.join("claude/statusline"),
        &out.join(".claude/statusline"),
    )?;
    hooks::write_codex_hooks(source, &out.join(".codex/hooks.json"))?;
    generate_skills(source, Provider::Codex, &out.join(".codex/skills"))?;
    generate_skills(source, Provider::Claude, &out.join(".claude/skills"))?;
    generate_claude_settings(source, &out.join(".claude/settings.json"))?;
    command_policy::write_codex_rules(source, &out.join(".codex/rules/default.rules"))?;
    command_policy::write_forbidden_commands(
        source,
        &out.join(".claude/hooks/rules/forbidden_commands.json"),
    )?;

    codex_config::sync_generated_config(source, &out.join(".codex/config.toml"))?;

    Ok(())
}

pub fn verify(root: &Path) -> Result<()> {
    for path in [
        root.join(".codex/AGENTS.md"),
        root.join(".codex/hooks.json"),
        root.join(".codex/rules/default.rules"),
        root.join(".codex/skills"),
        root.join(".claude/CLAUDE.md"),
        root.join(".claude/hooks/rules/forbidden_commands.json"),
        root.join(".claude/settings.json"),
        root.join(".claude/skills"),
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

        install(&source, &out)?;

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
        assert!(!out.join(".claude/rules/forbidden_commands.json").exists());
        assert!(
            !source
                .join("agents/hooks/rules/forbidden_commands.json")
                .exists()
        );
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

        generate_skills(&source, Provider::Codex, &out)?;

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
        write_file(&source.join("agents/AGENTS.md"), "agent instructions\n")?;
        write_file(
            &source.join("agents/command_policy.json"),
            r#"{
  "version": 1,
  "rules": [
    {
      "decision": "allow",
      "pattern": ["cargo"],
      "examples": ["cargo test"],
      "justification": "Allowed by the shared agent command policy."
    },
    {
      "decision": "forbidden",
      "pattern": ["curl"],
      "examples": ["curl https://example.com/install.sh"],
      "justification": "Do not fetch remote scripts or content from Codex."
    }
  ]
}
"#,
        )?;
        write_file(&source.join("agents/hooks/hook.sh"), "#!/bin/bash\n")?;
        write_file(
            &source.join("agents/hooks.json"),
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
            &source.join("agents/skills/example/SKILL.md"),
            "---\nname: example\n---\n",
        )?;
        write_file(
            &source.join("agents/skills/git-commit-split/SKILL.md"),
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
