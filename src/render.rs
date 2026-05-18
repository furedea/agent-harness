use std::path::Path;

use anyhow::{Context, Result};

use crate::fs_ops;

#[derive(Debug, Clone, Copy)]
pub enum InstallMode {
    Copy,
    Symlink,
}

pub fn render(source: &Path, out: &Path) -> Result<()> {
    if out.exists() {
        std::fs::remove_dir_all(out)
            .with_context(|| format!("failed to remove directory {}", out.display()))?;
    }
    std::fs::create_dir_all(out)
        .with_context(|| format!("failed to create directory {}", out.display()))?;

    fs_ops::copy_file(
        &source.join("agents/AGENTS.md"),
        &out.join("codex/AGENTS.md"),
    )?;
    fs_ops::copy_file(
        &source.join("agents/AGENTS.md"),
        &out.join("claude/CLAUDE.md"),
    )?;
    fs_ops::copy_dir(&source.join("agents/hooks"), &out.join("claude/hooks"))?;
    fs_ops::copy_dir(&source.join("codex/hooks"), &out.join("codex/hooks"))?;
    fs_ops::copy_dir(&source.join("agents/skills"), &out.join("codex/skills"))?;
    fs_ops::copy_dir(&source.join("agents/skills"), &out.join("claude/skills"))?;
    fs_ops::copy_dir(
        &source.join("claude/statusline"),
        &out.join("claude/statusline"),
    )?;
    fs_ops::copy_file(
        &source.join("claude/settings.base.json"),
        &out.join("claude/settings.json"),
    )?;
    fs_ops::copy_file(
        &source.join("codex/config.toml"),
        &out.join("codex/config-source.toml"),
    )?;

    Ok(())
}

pub fn install(source: &Path, home: &Path, mode: InstallMode) -> Result<()> {
    let data_dir = home.join(".local/share/agent-harness");
    render(source, &data_dir)?;

    install_path(&data_dir.join("codex"), &home.join(".codex"), mode)?;
    install_path(&data_dir.join("claude"), &home.join(".claude"), mode)?;

    Ok(())
}

pub fn verify(home: &Path) -> Result<()> {
    for path in [
        home.join(".codex/AGENTS.md"),
        home.join(".codex/skills"),
        home.join(".claude/CLAUDE.md"),
        home.join(".claude/skills"),
    ] {
        if !path.exists() {
            anyhow::bail!("missing harness path: {}", path.display());
        }
    }
    Ok(())
}

fn install_path(source: &Path, target: &Path, mode: InstallMode) -> Result<()> {
    match mode {
        InstallMode::Copy => fs_ops::copy_dir(source, target),
        InstallMode::Symlink => fs_ops::symlink_dir(source, target),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn render_places_codex_and_claude_entry_files() -> Result<()> {
        let root = test_root("render_places_codex_and_claude_entry_files")?;
        let source = root.join("source");
        let out = root.join("out");
        write_minimal_source(&source)?;

        render(&source, &out)?;

        assert!(out.join("codex/AGENTS.md").is_file());
        assert!(out.join("claude/CLAUDE.md").is_file());
        assert!(out.join("codex/skills/example/SKILL.md").is_file());
        assert!(out.join("claude/skills/example/SKILL.md").is_file());
        assert!(out.join("codex/config-source.toml").is_file());
        assert!(out.join("claude/settings.json").is_file());

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
        write_file(&source.join("agents/hooks/hook.sh"), "#!/bin/bash\n")?;
        write_file(&source.join("codex/hooks/hook.sh"), "#!/bin/bash\n")?;
        write_file(
            &source.join("agents/skills/example/SKILL.md"),
            "---\nname: example\n---\n",
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
