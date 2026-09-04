use std::path::Path;

use anyhow::{Context, Result};
use serde::Serialize;

use crate::{
    fs_ops,
    generation::{external_hooks::ExternalHookBundle, io},
    layout::{InstalledLayout, SourceLayout},
    runtime_root::RuntimeRoot,
};

const GLOB_SCAN_MAX_DEPTH: u64 = 5;
const POLICY_VERSION: u64 = 1;

#[derive(Debug, Serialize)]
struct ProtectedPathPolicy {
    version: u64,
    paths: Vec<String>,
}

pub(crate) fn write_codex_config_fragment(
    source: &Path,
    external_hooks: &[ExternalHookBundle],
    path: &Path,
) -> Result<()> {
    io::write_file(path, &codex_config_fragment(source, external_hooks)?)
}

pub(crate) fn write_runtime_policy(
    source: &Path,
    external_hooks: &[ExternalHookBundle],
    runtime_root: &RuntimeRoot,
    path: &Path,
) -> Result<()> {
    let policy = ProtectedPathPolicy {
        version: POLICY_VERSION,
        paths: protected_paths_for_runtime(source, external_hooks, runtime_root)?,
    };
    io::write_json(path, &policy)
}

pub(crate) fn codex_config_fragment(
    source: &Path,
    external_hooks: &[ExternalHookBundle],
) -> Result<String> {
    codex_config_fragment_for_runtime(source, external_hooks, &RuntimeRoot::home())
}

pub(crate) fn codex_config_fragment_for_runtime(
    source: &Path,
    external_hooks: &[ExternalHookBundle],
    runtime_root: &RuntimeRoot,
) -> Result<String> {
    let mut content = String::from("[permissions.guarded.filesystem]\n");

    for path in protected_paths_for_runtime(source, external_hooks, runtime_root)? {
        content.push_str(&format!("\"{}\" = \"read\"\n", toml_escape(&path)));
    }
    content.push_str(&format!("glob_scan_max_depth = {GLOB_SCAN_MAX_DEPTH}\n"));

    Ok(content)
}

#[cfg(test)]
pub(crate) fn protected_claude_deny_permissions(
    source: &Path,
    external_hooks: &[ExternalHookBundle],
) -> Result<Vec<String>> {
    Ok(protected_paths(source, external_hooks)?
        .into_iter()
        .map(|path| format!("Edit({path})"))
        .collect())
}

#[cfg(test)]
pub(crate) fn protected_paths(
    source: &Path,
    external_hooks: &[ExternalHookBundle],
) -> Result<Vec<String>> {
    protected_paths_for_runtime(source, external_hooks, &RuntimeRoot::home())
}

pub(crate) fn protected_paths_for_runtime(
    source: &Path,
    external_hooks: &[ExternalHookBundle],
    runtime_root: &RuntimeRoot,
) -> Result<Vec<String>> {
    let layout = SourceLayout::new(source);
    let agent_hooks = relative_files(&layout.agent_hooks())?;
    let codex_hooks = relative_files(&layout.codex_hooks())?;
    let mut paths = Vec::new();

    paths.extend(
        agent_hooks
            .iter()
            .map(|path| runtime_root.path(&Path::new(".claude/hooks").join(path))),
    );
    paths.extend(
        codex_hooks
            .iter()
            .map(|path| runtime_root.path(&Path::new(".codex/hooks").join(path))),
    );
    for bundle in external_hooks {
        paths.extend(
            bundle
                .asset_install_paths()?
                .into_iter()
                .map(|path| runtime_root.path(&path)),
        );
    }
    paths.extend(
        InstalledLayout::static_protected_paths()
            .iter()
            .map(|path| runtime_root.path(path)),
    );
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

        let paths = protected_paths(&root, &[])?;

        assert!(paths.contains(&"~/.claude/hooks/guard.sh".to_string()));
        assert!(paths.contains(&"~/.claude/hooks/rules/allowed_commands.json".to_string()));
        assert!(paths.contains(&"~/.claude/hooks/rules/command_permissions.json".to_string()));
        assert!(paths.contains(&"~/.claude/hooks/rules/forbidden_commands.json".to_string()));
        assert!(paths.contains(&"~/.claude/hooks/rules/protected_paths.json".to_string()));
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
    fn protected_claude_permissions_use_edit_for_file_modifications() -> Result<()> {
        let root = test_root("protected_claude_permissions_use_edit_for_file_modifications")?;
        write_minimal_source(&root)?;

        let permissions = protected_claude_deny_permissions(&root, &[])?;

        assert!(
            permissions
                .iter()
                .any(|permission| permission == "Edit(~/.claude/hooks/guard.sh)")
        );
        assert!(
            permissions
                .iter()
                .all(|permission| !permission.starts_with("Write("))
        );

        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn codex_config_fragment_writes_guarded_filesystem_toml() -> Result<()> {
        let root = test_root("codex_config_fragment_writes_guarded_filesystem_toml")?;
        write_minimal_source(&root)?;

        let content = codex_config_fragment(&root, &[])?;

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
        write_file(&root.join("hooks/docs/logs/audit/2026-05-19.jsonl"), "{}\n")?;

        let paths = protected_paths(&root, &[])?;

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
        write_file(
            &source.join("manifest.json"),
            r#"{"version":1,"runtime_commands":[]}"#,
        )?;
        write_file(&source.join("AGENTS.md"), "agent instructions\n")?;
        write_file(&source.join("hooks/guard.sh"), "#!/bin/bash\n")?;
        write_file(&source.join("hooks/rules/allowed_commands.json"), "{}\n")?;
        write_file(&source.join("hooks/rules/forbidden_commands.json"), "{}\n")?;
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
