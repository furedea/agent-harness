use std::fmt::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use serde::Serialize;

use crate::{
    file_op::FileOp,
    generation::{
        GenerationContext,
        permission_rule::{CodexFilesystemEntry, CodexScope},
        secret_path_policy,
    },
    layout::{InstalledLayout, SourceLayout},
    runtime_root::RuntimeRoot,
    source_model::Source,
};

const GLOB_SCAN_MAX_DEPTH: u64 = 5;
const POLICY_VERSION: u64 = 1;

#[derive(Debug, Serialize)]
struct ProtectedPathPolicy {
    version: u64,
    paths: Vec<String>,
}

pub(crate) fn codex_config_fragment_op(source: &Source, path: &Path) -> Result<FileOp> {
    Ok(FileOp::write(path, codex_config_fragment(source)?))
}

pub(crate) fn runtime_policy_op(paths: Vec<String>, path: &Path) -> Result<FileOp> {
    let policy = ProtectedPathPolicy {
        version: POLICY_VERSION,
        paths,
    };
    FileOp::write_json(path, &policy)
}

pub(crate) fn codex_config_fragment(source: &Source) -> Result<String> {
    codex_config_fragment_for_runtime(GenerationContext {
        source,
        runtime_root: &RuntimeRoot::home(),
        dsh_profile: "web",
    })
}

pub(crate) fn codex_config_fragment_for_runtime(context: GenerationContext<'_>) -> Result<String> {
    let entries = guarded_filesystem_entries(context)?;
    let mut fragment = String::from("[permissions.guarded.filesystem]\n");

    write_entries(&mut fragment, &entries, CodexScope::Absolute)?;
    writeln!(fragment, "glob_scan_max_depth = {GLOB_SCAN_MAX_DEPTH}")?;
    if entries
        .iter()
        .any(|entry| entry.scope == CodexScope::WorkspaceRoots)
    {
        fragment.push_str("\n[permissions.guarded.filesystem.\":workspace_roots\"]\n");
        write_entries(&mut fragment, &entries, CodexScope::WorkspaceRoots)?;
    }

    crate::generation::guarded_conflict::check(&fragment.parse()?, None)?;
    Ok(fragment)
}

/// Protected harness paths first, then secret paths. A path both name with different accesses
/// is a conflict, and so is any entry covered by a stricter one (see `guarded_conflict`).
fn guarded_filesystem_entries(context: GenerationContext<'_>) -> Result<Vec<CodexFilesystemEntry>> {
    let protected = protected_paths_for_runtime(context)?
        .into_iter()
        .map(CodexFilesystemEntry::protected);
    let secret = secret_path_policy::codex_filesystem_entries(context.source)?;

    let mut entries: Vec<CodexFilesystemEntry> = Vec::new();
    for entry in protected.chain(secret) {
        let known = entries
            .iter()
            .find(|known| known.scope == entry.scope && known.path == entry.path);
        known.map_or(Ok(()), |known| ensure_same_access(known, &entry))?;
        let is_new = known.is_none();
        entries.extend(is_new.then_some(entry));
    }
    Ok(entries)
}

fn ensure_same_access(known: &CodexFilesystemEntry, entry: &CodexFilesystemEntry) -> Result<()> {
    ensure!(
        known.access == entry.access,
        "guarded filesystem conflict: {:?} is both \"{}\" and \"{}\" in the generated config; \
         Codex applies the more specific entry, so change one of them",
        entry.path,
        known.access.as_str(),
        entry.access.as_str()
    );
    Ok(())
}

fn write_entries(
    fragment: &mut String,
    entries: &[CodexFilesystemEntry],
    scope: CodexScope,
) -> Result<()> {
    for entry in entries.iter().filter(|entry| entry.scope == scope) {
        writeln!(
            fragment,
            "{} = \"{}\"",
            toml_edit::Key::new(&entry.path),
            entry.access.as_str()
        )?;
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn protected_claude_deny_permissions(source: &Source) -> Result<Vec<String>> {
    Ok(protected_paths(source)?
        .into_iter()
        .map(|path| {
            crate::generation::permission_rule::PathRule::write_installed_path(path).claude()
        })
        .collect())
}

#[cfg(test)]
pub(crate) fn protected_paths(source: &Source) -> Result<Vec<String>> {
    protected_paths_for_runtime(GenerationContext {
        source,
        runtime_root: &RuntimeRoot::home(),
        dsh_profile: "web",
    })
}

pub(crate) fn protected_paths_for_runtime(context: GenerationContext<'_>) -> Result<Vec<String>> {
    protected_paths_for_providers(context, crate::provider::spec::PROVIDERS)
}

pub(crate) fn protected_paths_for_providers(
    context: GenerationContext<'_>,
    providers: &[crate::provider::spec::ProviderSpec],
) -> Result<Vec<String>> {
    let mut paths = Vec::new();
    for path in harness_owned_files(context, providers)? {
        push_unique(&mut paths, context.runtime_root.path(&path));
    }
    Ok(paths)
}

fn push_unique(paths: &mut Vec<String>, path: String) {
    if !paths.contains(&path) {
        paths.push(path);
    }
}

/// Every installed file the harness renders or syncs, except skills, relative to the root.
///
/// Derived from the same tables the installer uses, so a new provider output is protected
/// as soon as it is rendered.
fn harness_owned_files(
    context: GenerationContext<'_>,
    providers: &[crate::provider::spec::ProviderSpec],
) -> Result<Vec<PathBuf>> {
    let GenerationContext {
        source,
        dsh_profile,
        ..
    } = context;
    let installed = InstalledLayout::relative();
    let mut paths = Vec::new();
    for (source_dir, target_dir) in
        installed.synced_directories_for(SourceLayout::new(source.root()), providers)
    {
        let directory = source
            .directory(&source_dir)
            .with_context(|| format!("source directory {} was not loaded", source_dir.display()))?;
        paths.extend(
            directory
                .relative_files()
                .map(relative_text)
                .filter(|path| !is_runtime_artifact(path))
                .map(|path| target_dir.join(path)),
        );
    }
    paths.extend(
        providers
            .iter()
            .filter(|spec| spec.hook.bridge.directory.is_some())
            .flat_map(|spec| spec.hook.bridge.assets.iter())
            .map(|asset| PathBuf::from(asset.destination)),
    );
    for bundle in &source.hook_bundles {
        paths.extend(bundle.assets.iter().map(|asset| asset.target.clone()));
    }
    paths.extend(installed.generated_files_for(dsh_profile, providers));
    Ok(paths)
}

fn relative_text(path: &Path) -> String {
    let relative = path.to_string_lossy().into_owned();
    #[cfg(windows)]
    let relative = relative.replace('\\', "/");
    relative
}

fn is_runtime_artifact(path: &str) -> bool {
    path.starts_with("docs/logs/")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use crate::shell::fixtures::load_source;

    #[test]
    fn protected_paths_include_installed_harness_files_only() -> Result<()> {
        let root = test_root("protected_paths_include_installed_harness_files_only")?;
        write_minimal_source(&root)?;

        let paths = protected_paths(&load_source(&root)?)?;

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

        crate::shell::fixtures::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn protected_claude_permissions_use_edit_for_file_modifications() -> Result<()> {
        let root = test_root("protected_claude_permissions_use_edit_for_file_modifications")?;
        write_minimal_source(&root)?;

        let permissions = protected_claude_deny_permissions(&load_source(&root)?)?;

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

        crate::shell::fixtures::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn codex_config_fragment_writes_guarded_filesystem_toml() -> Result<()> {
        let root = test_root("codex_config_fragment_writes_guarded_filesystem_toml")?;
        write_minimal_source(&root)?;

        let content = codex_config_fragment(&load_source(&root)?)?;

        assert!(content.contains("[permissions.guarded.filesystem]"));
        assert!(content.contains("\"~/.claude/hooks/guard.sh\" = \"read\""));
        assert!(content.contains("glob_scan_max_depth = 5"));

        crate::shell::fixtures::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn codex_config_fragment_applies_the_secret_path_policy() -> Result<()> {
        let root = test_root("codex_config_fragment_applies_the_secret_path_policy")?;
        write_minimal_source(&root)?;
        write_file(
            &root.join("hooks/rules/secret_path_policy.json"),
            r#"{"version":1,"rules":[
{"pattern":"~/.ssh/**","access":["read","write"]},
{"pattern":"**/secrets/**","access":["read"]},
{"pattern":"~/.aws/credentials","access":["write"]},
{"pattern":"**/agent-harness/verification/**","access":["write"]},
{"pattern":"~/.codex/hooks.json","access":["write"]}
]}"#,
        )?;

        let content = codex_config_fragment(&load_source(&root)?)?;
        let (table, scoped) = content.split_once("\n[").unwrap();
        let lines: Vec<&str> = table.lines().collect();

        assert!(lines.contains(&"\"~/.codex/hooks/adapt.sh\" = \"read\""));
        assert!(lines.contains(&"\"~/.codex/hooks.json\" = \"read\""));
        assert_eq!(
            lines[lines.len() - 2..],
            ["\"~/.ssh/**\" = \"none\"", "glob_scan_max_depth = 5",],
        );
        assert!(!content.contains("~/.aws/credentials"));
        assert!(!content.contains("verification"));
        assert_eq!(
            scoped,
            "permissions.guarded.filesystem.\":workspace_roots\"]\n\"**/secrets/**\" = \"none\"\n",
        );
        content.parse::<toml_edit::DocumentMut>()?;
        crate::shell::fixtures::remove_dir_all(root)?;
        Ok(())
    }

    fn fragment_error(policy: &str) -> Result<String> {
        let root = test_root("fragment_error")?;
        write_minimal_source(&root)?;
        write_file(&root.join("hooks/rules/secret_path_policy.json"), policy)?;
        let error = codex_config_fragment(&load_source(&root)?).unwrap_err();
        crate::shell::fixtures::remove_dir_all(root)?;
        Ok(format!("{error:#}"))
    }

    #[test]
    fn a_secret_rule_on_a_protected_path_with_another_access_is_a_conflict() -> Result<()> {
        let message = fragment_error(
            r#"{"version":1,"rules":[{"pattern":"~/.codex/hooks.json","access":["read"]}]}"#,
        )?;

        assert!(message.contains("guarded filesystem conflict"), "{message}");
        assert!(message.contains("~/.codex/hooks.json"), "{message}");
        Ok(())
    }

    #[test]
    fn a_generated_denial_over_a_protected_path_is_a_conflict() -> Result<()> {
        let message = fragment_error(
            r#"{"version":1,"rules":[{"pattern":"~/.codex/**","access":["read"]}]}"#,
        )?;

        assert!(
            message.contains("~/.codex/** = \"none\" (generated)"),
            "{message}"
        );
        assert!(
            message.contains("~/.codex/hooks/adapt.sh = \"read\" (generated)"),
            "{message}"
        );
        Ok(())
    }

    #[test]
    fn overlapping_generated_entries_with_the_same_access_pass() -> Result<()> {
        let root = test_root("overlapping_generated_entries_with_the_same_access_pass")?;
        write_minimal_source(&root)?;
        write_file(
            &root.join("hooks/rules/secret_path_policy.json"),
            r#"{"version":1,"rules":[
{"pattern":"secrets/**","access":["read"]},
{"pattern":"secrets/key","access":["read"]}
]}"#,
        )?;

        let content = codex_config_fragment(&load_source(&root)?)?;

        assert!(content.contains("\"secrets/key\" = \"none\""));
        crate::shell::fixtures::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn a_write_only_rule_produces_no_codex_entry() -> Result<()> {
        let root = test_root("a_write_only_rule_produces_no_codex_entry")?;
        write_minimal_source(&root)?;
        write_file(
            &root.join("hooks/rules/secret_path_policy.json"),
            r#"{"version":1,"rules":[
{"pattern":".env*","access":["write"]},
{"pattern":"**/agent-harness/audit/**","access":["write"]}
]}"#,
        )?;

        let with_rules = codex_config_fragment(&load_source(&root)?)?;
        write_file(
            &root.join("hooks/rules/secret_path_policy.json"),
            r#"{"version":1,"rules":[]}"#,
        )?;
        let without_rules = codex_config_fragment(&load_source(&root)?)?;

        assert_eq!(with_rules, without_rules);
        crate::shell::fixtures::remove_dir_all(root)?;
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn protection_preserves_special_characters_in_the_runtime_root() -> Result<()> {
        let root = test_root("runtime-root-special-characters")?;
        write_minimal_source(&root)?;
        let runtime_path = PathBuf::from("/work/project\\name\nquoted\"directory");
        let runtime_root = RuntimeRoot::directory(runtime_path.clone())?;
        let loaded = load_source(&root)?;
        let expected = runtime_path.join(".claude/hooks/guard.sh");
        let expected = expected.to_str().unwrap();

        let content = codex_config_fragment_for_runtime(GenerationContext {
            source: &loaded,
            runtime_root: &runtime_root,
            dsh_profile: "web",
        })?;
        let document = content.parse::<toml_edit::DocumentMut>()?;

        assert_eq!(
            document["permissions"]["guarded"]["filesystem"][expected].as_str(),
            Some("read"),
        );
        assert!(
            protected_paths_for_runtime(GenerationContext {
                source: &loaded,
                runtime_root: &runtime_root,
                dsh_profile: "web"
            })?
            .contains(&expected.to_owned())
        );
        crate::shell::fixtures::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn protected_paths_ignore_hook_runtime_logs() -> Result<()> {
        let root = test_root("protected_paths_ignore_hook_runtime_logs")?;
        write_minimal_source(&root)?;
        write_file(&root.join("hooks/docs/logs/audit/2026-05-19.jsonl"), "{}\n")?;

        let paths = protected_paths(&load_source(&root)?)?;

        assert!(
            !paths
                .iter()
                .any(|path| path.contains("docs/logs/audit/2026-05-19.jsonl"))
        );

        crate::shell::fixtures::remove_dir_all(root)?;
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn protected_paths_preserve_backslashes_in_hook_filenames() -> Result<()> {
        let root = test_root("hook-filename-backslash")?;
        write_file(&root.join("hooks/guard\\name.sh"), "#!/bin/sh\n")?;

        let paths = protected_paths(&load_source(&root)?)?;

        assert!(paths.contains(&"~/.claude/hooks/guard\\name.sh".to_owned()));
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
        write_file(&source.join("hooks/guard.sh"), "#!/bin/bash\n")?;
        write_file(&source.join("codex/hooks/adapt.sh"), "#!/bin/bash\n")?;
        Ok(())
    }

    fn write_file(path: &Path, content: &str) -> Result<()> {
        path.parent()
            .map(crate::shell::fixtures::create_dir_all)
            .transpose()?;
        crate::shell::fixtures::write(path, content)?;
        Ok(())
    }
}
