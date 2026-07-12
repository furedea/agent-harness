use std::path::Path;

use anyhow::{Context, Result};

use crate::{fs_ops, generation::io};

const GLOB_SCAN_MAX_DEPTH: u64 = 5;

pub(crate) fn write_codex_config_fragment(source: &Path, path: &Path) -> Result<()> {
    io::write_file(path, &codex_config_fragment(source)?)
}

pub(crate) fn codex_config_fragment(source: &Path) -> Result<String> {
    let mut content = String::from("[permissions.guarded.filesystem]\n");

    for path in protected_paths(source)? {
        content.push_str(&format!("\"{}\" = \"read\"\n", toml_escape(&path)));
    }
    content.push_str(&format!("glob_scan_max_depth = {GLOB_SCAN_MAX_DEPTH}\n"));

    Ok(content)
}

pub(crate) fn protected_claude_deny_permissions(source: &Path) -> Result<Vec<String>> {
    let paths = protected_paths(source)?;
    let mut permissions = Vec::with_capacity(paths.len() * 2);

    for path in paths {
        permissions.push(format!("Edit({path})"));
        permissions.push(format!("Write({path})"));
    }

    Ok(permissions)
}

pub(crate) fn protected_paths(source: &Path) -> Result<Vec<String>> {
    let agent_hooks = relative_files(&source.join("agents/hooks"))?;
    let codex_hooks = relative_files(&source.join("codex/hooks"))?;
    let mut paths = Vec::new();

    paths.extend(home_agent_hook_paths(&agent_hooks));
    paths.extend(home_codex_hook_paths(&codex_hooks));
    paths.extend([
        "~/.claude/CLAUDE.md".to_string(),
        "~/.claude/hooks/rules/forbidden_commands.json".to_string(),
        "~/.claude/settings.json".to_string(),
        "~/.codex/AGENTS.md".to_string(),
        "~/.codex/hooks.json".to_string(),
        "~/.codex/rules/default.rules".to_string(),
    ]);
    Ok(paths)
}

fn relative_files(root: &Path) -> Result<Vec<String>> {
    fs_ops::regular_files(root)?
        .into_iter()
        .map(|path| {
            let relative = path
                .strip_prefix(root)
                .with_context(|| format!("failed to strip prefix {}", root.display()))?;
            Ok(relative.to_string_lossy().replace('\\', "/"))
        })
        .filter(|path| path.as_ref().is_ok_and(|path| !is_runtime_artifact(path)))
        .collect()
}

fn is_runtime_artifact(path: &str) -> bool {
    path.starts_with("docs/logs/")
}

fn home_agent_hook_paths(paths: &[String]) -> Vec<String> {
    paths
        .iter()
        .map(|path| format!("~/.claude/hooks/{path}"))
        .collect()
}

fn home_codex_hook_paths(paths: &[String]) -> Vec<String> {
    paths
        .iter()
        .map(|path| format!("~/.codex/hooks/{path}"))
        .collect()
}

fn toml_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn protected_paths_include_installed_harness_files_only() -> Result<()> {
        let root = test_root("protected_paths_include_installed_harness_files_only")?;
        write_minimal_source(&root)?;

        let paths = protected_paths(&root)?;

        assert!(paths.contains(&"~/.claude/hooks/guard.sh".to_string()));
        assert!(paths.contains(&"~/.claude/hooks/rules/forbidden_commands.json".to_string()));
        assert!(paths.contains(&"~/.codex/hooks/adapt.sh".to_string()));
        assert!(paths.contains(&"~/.codex/hooks.json".to_string()));
        assert!(
            !paths
                .iter()
                .any(|path| path.starts_with(&root.display().to_string()))
        );

        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn codex_config_fragment_writes_guarded_filesystem_toml() -> Result<()> {
        let root = test_root("codex_config_fragment_writes_guarded_filesystem_toml")?;
        write_minimal_source(&root)?;

        let content = codex_config_fragment(&root)?;

        assert!(content.contains("[permissions.guarded.filesystem]"));
        assert!(content.contains("\"~/.claude/hooks/guard.sh\" = \"read\""));
        assert!(content.contains("glob_scan_max_depth = 5"));

        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn protected_paths_ignore_hook_runtime_logs() -> Result<()> {
        let root = test_root("protected_paths_ignore_hook_runtime_logs")?;
        write_minimal_source(&root)?;
        write_file(
            &root.join("agents/hooks/docs/logs/audit/2026-05-19.jsonl"),
            "{}\n",
        )?;

        let paths = protected_paths(&root)?;

        assert!(
            !paths
                .iter()
                .any(|path| path.contains("docs/logs/audit/2026-05-19.jsonl"))
        );

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
        write_file(&source.join("agents/hooks/guard.sh"), "#!/bin/bash\n")?;
        write_file(&source.join("codex/hooks/adapt.sh"), "#!/bin/bash\n")?;
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
