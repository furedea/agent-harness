use std::fmt::Write;
use std::path::Path;

use anyhow::Result;

use crate::{
    file_op::FileOp,
    generation::{
        GenerationContext,
        permission_rule::{CodexFilesystemEntry, CodexScope},
        secret_path_policy,
    },
    runtime_root::RuntimeRoot,
    source_model::Source,
};

const GLOB_SCAN_MAX_DEPTH: u64 = 5;

pub(crate) fn codex_config_fragment_op(source: &Source, path: &Path) -> Result<FileOp> {
    Ok(FileOp::write(path, codex_config_fragment(source)?))
}

pub(crate) fn codex_config_fragment(source: &Source) -> Result<String> {
    codex_config_fragment_for_runtime(GenerationContext {
        source,
        runtime_root: &RuntimeRoot::home(),
    })
}

/// The Codex `[permissions.guarded.filesystem]` entries the source secret path policy requires.
pub(crate) fn codex_config_fragment_for_runtime(context: GenerationContext<'_>) -> Result<String> {
    let entries = guarded_filesystem_entries(context.source)?;
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

    crate::generation::guarded_conflict::check(
        &fragment.parse()?,
        None,
        context.runtime_root.project_directory().as_deref(),
    )?;
    Ok(fragment)
}

/// The secret path entries, each path once; a stricter entry covering another is a conflict
/// (see `guarded_conflict`).
fn guarded_filesystem_entries(source: &Source) -> Result<Vec<CodexFilesystemEntry>> {
    let mut entries: Vec<CodexFilesystemEntry> = Vec::new();
    for entry in secret_path_policy::codex_filesystem_entries(source)? {
        let is_new = !entries
            .iter()
            .any(|known| known.scope == entry.scope && known.path == entry.path);
        entries.extend(is_new.then_some(entry));
    }
    Ok(entries)
}

fn write_entries(
    fragment: &mut String,
    entries: &[CodexFilesystemEntry],
    scope: CodexScope,
) -> Result<()> {
    for entry in entries.iter().filter(|entry| entry.scope == scope) {
        writeln!(fragment, "{} = \"none\"", toml_edit::Key::new(&entry.path))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use crate::shell::fixtures::load_source;

    #[test]
    fn a_source_without_secret_rules_gets_only_the_scan_depth() -> Result<()> {
        let root = test_root("a_source_without_secret_rules_gets_only_the_scan_depth")?;
        write_minimal_source(&root)?;

        let content = codex_config_fragment(&load_source(&root)?)?;

        assert_eq!(
            content,
            "[permissions.guarded.filesystem]\nglob_scan_max_depth = 5\n"
        );

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

        assert_eq!(
            lines,
            [
                "[permissions.guarded.filesystem]",
                "\"~/.ssh/**\" = \"none\"",
                "glob_scan_max_depth = 5",
            ],
        );
        assert!(!content.contains("~/.codex/hooks.json"));
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

    #[test]
    fn overlapping_generated_entries_with_the_same_access_pass() -> Result<()> {
        let root = test_root("overlapping_generated_entries_with_the_same_access_pass")?;
        write_minimal_source(&root)?;
        write_file(
            &root.join("hooks/rules/secret_path_policy.json"),
            r#"{"version":1,"rules":[
{"pattern":"**/secrets/**","access":["read"]},
{"pattern":"**/secrets/key","access":["read"]}
]}"#,
        )?;

        let content = codex_config_fragment(&load_source(&root)?)?;

        assert!(content.contains("\"**/secrets/key\" = \"none\""));
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
