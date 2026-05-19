use std::path::{Component, Path};

use anyhow::{Context, Result, bail};

use crate::{fs_ops, render::Provider};

const CODEX_EXPLICIT_ONLY: &str = "policy:\n  allow_implicit_invocation: false\n";

enum FrontmatterValue {
    Bool(bool),
    String(&'static str),
}

struct ExtraFile {
    relative_path: &'static str,
    content: &'static str,
}

/// Render provider-specific skills into the output directory.
///
/// # Errors
///
/// Returns an error when source skills cannot be read, frontmatter contains
/// provider-specific keys, or rendered files cannot be written.
pub fn render_skills(source: &Path, provider: Provider, out: &Path) -> Result<()> {
    let skills_dir = source.join("agents/skills");
    if out.exists() {
        std::fs::remove_dir_all(out)
            .with_context(|| format!("failed to remove directory {}", out.display()))?;
    }
    std::fs::create_dir_all(out)
        .with_context(|| format!("failed to create directory {}", out.display()))?;

    for entry in sorted_skill_dirs(&skills_dir)? {
        render_skill_dir(
            &entry,
            provider,
            &out.join(entry.file_name().unwrap_or_default()),
        )?;
    }

    Ok(())
}

fn sorted_skill_dirs(skills_dir: &Path) -> Result<Vec<std::path::PathBuf>> {
    let mut dirs = Vec::new();
    for entry in std::fs::read_dir(skills_dir)
        .with_context(|| format!("failed to read directory {}", skills_dir.display()))?
    {
        let entry =
            entry.with_context(|| format!("failed to read entry in {}", skills_dir.display()))?;
        let path = entry.path();
        if entry
            .file_type()
            .with_context(|| format!("failed to inspect {}", path.display()))?
            .is_dir()
            && path.join("SKILL.md").is_file()
        {
            dirs.push(path);
        }
    }
    dirs.sort();
    Ok(dirs)
}

fn render_skill_dir(source: &Path, provider: Provider, out: &Path) -> Result<()> {
    let source_skill = source.join("SKILL.md");
    let content = std::fs::read_to_string(&source_skill)
        .with_context(|| format!("failed to read {}", source_skill.display()))?;
    let (frontmatter, body) = split_frontmatter(&content)?;
    let common_entries = common_frontmatter_entries(&source_skill, frontmatter)?;
    let skill_name = source
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();

    copy_support_files(source, out)?;
    write_skill(out, &common_entries, skill_name, provider, body)?;
    write_extra_files(out, skill_name, provider)
}

fn split_frontmatter(content: &str) -> Result<(&str, &str)> {
    if !content.starts_with("---\n") {
        bail!("SKILL.md must start with YAML frontmatter");
    }

    let marker = "\n---\n";
    let Some(end) = content["---\n".len()..].find(marker) else {
        bail!("SKILL.md frontmatter must end with ---");
    };
    let end = end + "---\n".len();

    Ok((&content["---\n".len()..end], &content[end + marker.len()..]))
}

fn common_frontmatter_entries(path: &Path, frontmatter: &str) -> Result<Vec<String>> {
    let entries = split_entries(frontmatter)?;
    let unknown_keys: Vec<&str> = entries
        .iter()
        .map(|(key, _entry)| key.as_str())
        .filter(|key| !matches!(*key, "name" | "description"))
        .collect();

    if !unknown_keys.is_empty() {
        bail!(
            "{}: non-common frontmatter keys must move to Rust skill overrides: {}",
            path.display(),
            unknown_keys.join(", "),
        );
    }

    Ok(entries.into_iter().map(|(_key, entry)| entry).collect())
}

fn split_entries(frontmatter: &str) -> Result<Vec<(String, String)>> {
    let mut entries: Vec<(String, Vec<String>)> = Vec::new();
    let mut current_key: Option<String> = None;
    let mut current_lines: Vec<String> = Vec::new();

    for line in frontmatter.lines() {
        if let Some(key) = frontmatter_key(line) {
            if let Some(previous_key) = current_key.replace(key.to_string()) {
                entries.push((previous_key, current_lines));
            }
            current_lines = vec![line.to_string()];
            continue;
        }

        if current_key.is_none() && !line.trim().is_empty() {
            bail!("frontmatter line is not under a key: {line}");
        }
        current_lines.push(line.to_string());
    }

    if let Some(key) = current_key {
        entries.push((key, current_lines));
    }

    Ok(entries
        .into_iter()
        .map(|(key, lines)| (key, lines.join("\n").trim_end().to_string()))
        .collect())
}

fn frontmatter_key(line: &str) -> Option<&str> {
    let (key, _value) = line.split_once(':')?;
    if key.is_empty()
        || !key
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
    {
        return None;
    }
    Some(key)
}

fn copy_support_files(source: &Path, out: &Path) -> Result<()> {
    std::fs::create_dir_all(out)
        .with_context(|| format!("failed to create directory {}", out.display()))?;

    for entry in std::fs::read_dir(source)
        .with_context(|| format!("failed to read directory {}", source.display()))?
    {
        let entry =
            entry.with_context(|| format!("failed to read entry in {}", source.display()))?;
        let path = entry.path();
        let name = entry.file_name();
        if name == "SKILL.md" {
            continue;
        }

        let target = out.join(name);
        let file_type = entry
            .file_type()
            .with_context(|| format!("failed to inspect {}", path.display()))?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            fs_ops::copy_dir(&path, &target)?;
        } else if file_type.is_file() {
            fs_ops::copy_file(&path, &target)?;
        }
    }

    Ok(())
}

fn write_skill(
    out: &Path,
    common_entries: &[String],
    skill_name: &str,
    provider: Provider,
    body: &str,
) -> Result<()> {
    let mut frontmatter = common_entries.to_vec();
    for (key, value) in frontmatter_overrides(skill_name, provider) {
        frontmatter.push(frontmatter_entry(key, value)?);
    }

    let content = format!("---\n{}\n---\n{}", frontmatter.join("\n").trim_end(), body);
    std::fs::write(out.join("SKILL.md"), content)
        .with_context(|| format!("failed to write {}", out.join("SKILL.md").display()))
}

fn frontmatter_overrides(
    skill_name: &str,
    provider: Provider,
) -> Vec<(&'static str, FrontmatterValue)> {
    match (skill_name, provider) {
        ("git-commit-split", Provider::Claude) => vec![
            (
                "argument-hint",
                FrontmatterValue::String("{direct | pr-per-feature}"),
            ),
            ("disable-model-invocation", FrontmatterValue::Bool(true)),
        ],
        ("git-commit-split", Provider::Codex) => {
            vec![(
                "argument-hint",
                FrontmatterValue::String("{direct | pr-per-feature}"),
            )]
        }
        ("github-ci-init" | "nix-dev-init" | "skill-auditor", Provider::Claude) => {
            vec![("disable-model-invocation", FrontmatterValue::Bool(true))]
        }
        _ => Vec::new(),
    }
}

fn frontmatter_entry(key: &str, value: FrontmatterValue) -> Result<String> {
    let rendered = match value {
        FrontmatterValue::Bool(value) => value.to_string(),
        FrontmatterValue::String(value) => serde_json::to_string(value)?,
    };
    Ok(format!("{key}: {rendered}"))
}

fn write_extra_files(out: &Path, skill_name: &str, provider: Provider) -> Result<()> {
    for file in extra_files(skill_name, provider) {
        let target = resolve_extra_file_path(out, file.relative_path)?;
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create directory {}", parent.display()))?;
        }
        std::fs::write(&target, file.content)
            .with_context(|| format!("failed to write {}", target.display()))?;
    }
    Ok(())
}

fn extra_files(skill_name: &str, provider: Provider) -> Vec<ExtraFile> {
    match (skill_name, provider) {
        (
            "git-commit-split" | "github-ci-init" | "nix-dev-init" | "skill-auditor",
            Provider::Codex,
        ) => vec![ExtraFile {
            relative_path: "agents/openai.yaml",
            content: CODEX_EXPLICIT_ONLY,
        }],
        _ => Vec::new(),
    }
}

fn resolve_extra_file_path(out: &Path, relative_path: &str) -> Result<std::path::PathBuf> {
    let path = Path::new(relative_path);
    if path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        bail!("extra file path must be relative and stay within the skill: {relative_path}");
    }
    Ok(out.join(path))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn render_claude_skill_applies_frontmatter_overrides() -> Result<()> {
        let root = test_root("render_claude_skill_applies_frontmatter_overrides")?;
        write_skill_source(&root, "git-commit-split")?;
        let out = root.join("out");

        render_skills(&root, Provider::Claude, &out)?;

        let content = std::fs::read_to_string(out.join("git-commit-split/SKILL.md"))?;
        assert!(content.contains("argument-hint: \"{direct | pr-per-feature}\""));
        assert!(content.contains("disable-model-invocation: true"));
        assert!(!out.join("git-commit-split/agents/openai.yaml").exists());

        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn render_codex_skill_writes_explicit_only_metadata() -> Result<()> {
        let root = test_root("render_codex_skill_writes_explicit_only_metadata")?;
        write_skill_source(&root, "github-ci-init")?;
        let out = root.join("out");

        render_skills(&root, Provider::Codex, &out)?;

        let content = std::fs::read_to_string(out.join("github-ci-init/SKILL.md"))?;
        assert!(!content.contains("disable-model-invocation"));
        assert_eq!(
            std::fs::read_to_string(out.join("github-ci-init/agents/openai.yaml"))?,
            CODEX_EXPLICIT_ONLY,
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

    fn write_skill_source(source: &Path, skill_name: &str) -> Result<()> {
        let skill_dir = source.join("agents/skills").join(skill_name);
        std::fs::create_dir_all(&skill_dir)?;
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: example\ndescription: test skill\n---\n\nBody\n",
        )?;
        Ok(())
    }
}
