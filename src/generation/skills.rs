use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::{fs_ops, generation::io, render::Provider};

const SKILL_RENDERING_PATH: &str = "agents/skill_rendering.json";
const SUPPORTED_SKILL_RENDERING_VERSION: u64 = 1;
const CODEX_OPENAI_PATH: &str = "agents/openai.yaml";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SkillMetadata {
    pub(crate) name: String,
    pub(crate) description: String,
}

#[derive(Clone, Debug)]
pub(crate) struct ExternalSkill {
    name: SkillName,
    source: PathBuf,
}

impl ExternalSkill {
    pub(crate) fn new(name: impl Into<String>, source: PathBuf) -> Result<Self> {
        Ok(Self {
            name: SkillName::parse(name)?,
            source,
        })
    }
}

impl FromStr for ExternalSkill {
    type Err = String;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        let (name, source) = value
            .split_once('=')
            .ok_or_else(|| "external skill must use NAME=PATH format".to_string())?;
        if source.is_empty() {
            return Err("external skill path must not be empty".to_string());
        }
        Self::new(name, PathBuf::from(source)).map_err(|error| error.to_string())
    }
}

#[derive(Clone, Debug)]
struct SkillName(String);

impl SkillName {
    fn parse(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        let is_valid = !value.is_empty()
            && value
                .chars()
                .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-');
        if !is_valid {
            bail!(
                "external skill name must contain only lowercase ASCII letters, digits, and hyphens: {value}"
            );
        }
        Ok(Self(value))
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SkillRendering {
    version: u64,
    #[serde(default)]
    skills: BTreeMap<String, SkillRenderingEntry>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SkillRenderingEntry {
    #[serde(default)]
    claude: ClaudeRendering,
    #[serde(default)]
    codex: CodexRendering,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ClaudeRendering {
    #[serde(default)]
    frontmatter: BTreeMap<String, FrontmatterValue>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct CodexRendering {
    #[serde(default)]
    frontmatter: BTreeMap<String, FrontmatterValue>,
    openai: Option<CodexOpenAi>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CodexOpenAi {
    allow_implicit_invocation: bool,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum FrontmatterValue {
    Bool(bool),
    String(String),
}

pub(crate) fn render_skills(
    source: &Path,
    provider: Provider,
    external_skills: &[ExternalSkill],
    out: &Path,
) -> Result<()> {
    let skills_dir = source.join("agents/skills");
    let skill_dirs = sorted_skill_dirs(&skills_dir)?;
    let skill_rendering = read_skill_rendering(source)?;
    validate_skill_rendering_targets(&skill_rendering, &skill_dirs)?;
    validate_external_skills(&skill_dirs, external_skills)?;

    if out.exists() {
        std::fs::remove_dir_all(out)
            .with_context(|| format!("failed to remove directory {}", out.display()))?;
    }
    std::fs::create_dir_all(out)
        .with_context(|| format!("failed to create directory {}", out.display()))?;

    for entry in skill_dirs {
        render_skill_dir(
            &entry,
            &skill_rendering,
            provider,
            &out.join(entry.file_name().unwrap_or_default()),
        )?;
    }

    for skill in external_skills {
        fs_ops::copy_dir(&skill.source, &out.join(skill.name.as_str()))?;
    }

    Ok(())
}

pub(crate) fn built_in_skill_metadata(source: &Path) -> Result<Vec<SkillMetadata>> {
    let skills_dir = source.join("agents/skills");
    let mut metadata = sorted_skill_dirs(&skills_dir)?
        .iter()
        .map(|skill_dir| read_skill_metadata(&skill_dir.join("SKILL.md")))
        .collect::<Result<Vec<_>>>()?;
    metadata.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(metadata)
}

fn sorted_skill_dirs(skills_dir: &Path) -> Result<Vec<PathBuf>> {
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

fn render_skill_dir(
    source: &Path,
    skill_rendering: &SkillRendering,
    provider: Provider,
    out: &Path,
) -> Result<()> {
    let source_skill = source.join("SKILL.md");
    let content = std::fs::read_to_string(&source_skill)
        .with_context(|| format!("failed to read {}", source_skill.display()))?;
    let (frontmatter, body) = split_frontmatter(&content)?;
    let common_entries = common_frontmatter_entries(&source_skill, frontmatter)?;
    let skill_name = source
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();

    let empty_rendering = SkillRenderingEntry::default();
    let rendering = skill_rendering
        .skills
        .get(skill_name)
        .unwrap_or(&empty_rendering);

    copy_support_files(source, out)?;
    write_skill(out, &common_entries, rendering, provider, body)?;
    write_extra_files(out, rendering, provider)
}

fn read_skill_rendering(source: &Path) -> Result<SkillRendering> {
    let path = source.join(SKILL_RENDERING_PATH);
    if !path.exists() {
        return Ok(SkillRendering::default());
    }

    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let rendering: SkillRendering = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    if rendering.version != SUPPORTED_SKILL_RENDERING_VERSION {
        bail!("unsupported skill rendering version: {}", rendering.version);
    }
    Ok(rendering)
}

fn validate_skill_rendering_targets(
    skill_rendering: &SkillRendering,
    skill_dirs: &[PathBuf],
) -> Result<()> {
    let skill_names = skill_dirs
        .iter()
        .filter_map(|path| path.file_name().and_then(|name| name.to_str()))
        .collect::<BTreeSet<_>>();
    let unknown = skill_rendering
        .skills
        .keys()
        .filter(|name| !skill_names.contains(name.as_str()))
        .cloned()
        .collect::<Vec<_>>();

    if unknown.is_empty() {
        return Ok(());
    }

    bail!(
        "{SKILL_RENDERING_PATH} references unknown skills: {}",
        unknown.join(", ")
    )
}

fn validate_external_skills(
    skill_dirs: &[PathBuf],
    external_skills: &[ExternalSkill],
) -> Result<()> {
    let built_in_names = skill_dirs
        .iter()
        .filter_map(|path| path.file_name().and_then(|name| name.to_str()))
        .collect::<BTreeSet<_>>();
    let mut external_names = BTreeSet::new();

    for skill in external_skills {
        if !skill.source.join("SKILL.md").is_file() {
            bail!(
                "external skill directory must contain SKILL.md: {}",
                skill.source.display()
            );
        }
        if built_in_names.contains(skill.name.as_str()) {
            bail!(
                "external skill shadows a built-in skill: {}",
                skill.name.as_str()
            );
        }
        if !external_names.insert(skill.name.as_str()) {
            bail!("duplicate external skill name: {}", skill.name.as_str());
        }
    }

    Ok(())
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

fn read_skill_metadata(path: &Path) -> Result<SkillMetadata> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let (frontmatter, _body) = split_frontmatter(&content)?;
    let entries = split_entries(frontmatter)?;
    let name = frontmatter_text(&entries, "name")?;
    let description = frontmatter_text(&entries, "description")?;
    Ok(SkillMetadata { name, description })
}

fn frontmatter_text(entries: &[(String, String)], target: &str) -> Result<String> {
    let entry = entries
        .iter()
        .find_map(|(name, entry)| (name == target).then_some(entry))
        .with_context(|| format!("SKILL.md frontmatter must contain {target}"))?;
    let (_name, first_line) = entry
        .lines()
        .next()
        .and_then(|line| line.split_once(':'))
        .with_context(|| format!("SKILL.md frontmatter {target} is invalid"))?;
    let first_line = first_line.trim();
    let value = if matches!(first_line, ">" | ">-" | "|" | "|-") {
        entry
            .lines()
            .skip(1)
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        first_line.trim_matches('"').to_owned()
    };
    if value.is_empty() {
        bail!("SKILL.md frontmatter {target} must not be empty");
    }
    Ok(value)
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
    skill_rendering: &SkillRenderingEntry,
    provider: Provider,
    body: &str,
) -> Result<()> {
    let mut frontmatter = common_entries.to_vec();
    for (key, value) in frontmatter_overrides(skill_rendering, provider) {
        frontmatter.push(frontmatter_entry(key, value)?);
    }

    let content = format!("---\n{}\n---\n{}", frontmatter.join("\n").trim_end(), body);
    io::write_file(&out.join("SKILL.md"), &content)
}

fn frontmatter_overrides(
    skill_rendering: &SkillRenderingEntry,
    provider: Provider,
) -> &BTreeMap<String, FrontmatterValue> {
    match provider {
        Provider::Claude => &skill_rendering.claude.frontmatter,
        Provider::Codex => &skill_rendering.codex.frontmatter,
    }
}

fn frontmatter_entry(key: &str, value: &FrontmatterValue) -> Result<String> {
    validate_frontmatter_key(key)?;
    let rendered = match value {
        FrontmatterValue::Bool(value) => value.to_string(),
        FrontmatterValue::String(value) => serde_json::to_string(value)?,
    };
    Ok(format!("{key}: {rendered}"))
}

fn validate_frontmatter_key(key: &str) -> Result<()> {
    if key.is_empty()
        || !key
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
    {
        bail!("skill rendering frontmatter key is invalid: {key}");
    }
    Ok(())
}

fn write_extra_files(
    out: &Path,
    skill_rendering: &SkillRenderingEntry,
    provider: Provider,
) -> Result<()> {
    if let Provider::Codex = provider
        && let Some(openai) = &skill_rendering.codex.openai
    {
        io::write_file(&out.join(CODEX_OPENAI_PATH), &codex_openai_content(openai))?;
    }
    Ok(())
}

fn codex_openai_content(openai: &CodexOpenAi) -> String {
    format!(
        "policy:\n  allow_implicit_invocation: {}\n",
        openai.allow_implicit_invocation
    )
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn render_claude_skill_applies_frontmatter_overrides() -> Result<()> {
        let root = test_root("render_claude_skill_applies_frontmatter_overrides")?;
        write_skill_source(&root, "custom-command")?;
        write_skill_rendering(
            &root,
            r#"{
  "version": 1,
  "skills": {
    "custom-command": {
      "claude": {
        "frontmatter": {
          "argument-hint": "{direct | pr-per-feature}",
          "disable-model-invocation": true
        }
      }
    }
  }
}
"#,
        )?;
        let out = root.join("out");

        render_skills(&root, Provider::Claude, &[], &out)?;

        let content = std::fs::read_to_string(out.join("custom-command/SKILL.md"))?;
        assert!(content.contains("argument-hint: \"{direct | pr-per-feature}\""));
        assert!(content.contains("disable-model-invocation: true"));
        assert!(!out.join("custom-command/agents/openai.yaml").exists());

        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn render_codex_skill_writes_explicit_only_metadata() -> Result<()> {
        let root = test_root("render_codex_skill_writes_explicit_only_metadata")?;
        write_skill_source(&root, "custom-command")?;
        write_skill_rendering(
            &root,
            r#"{
  "version": 1,
  "skills": {
    "custom-command": {
      "codex": {
        "openai": {
          "allow_implicit_invocation": false
        }
      }
    }
  }
}
"#,
        )?;
        let out = root.join("out");

        render_skills(&root, Provider::Codex, &[], &out)?;

        let content = std::fs::read_to_string(out.join("custom-command/SKILL.md"))?;
        assert!(!content.contains("disable-model-invocation"));
        assert_eq!(
            std::fs::read_to_string(out.join("custom-command/agents/openai.yaml"))?,
            "policy:\n  allow_implicit_invocation: false\n",
        );

        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn render_skills_copies_an_external_skill_directory_verbatim() -> Result<()> {
        let root = test_root("render_skills_copies_an_external_skill_directory_verbatim")?;
        write_skill_source(&root, "built-in")?;
        let external_skill_dir = root.join("external/herdr");
        let external_skill_content =
            "---\nname: herdr\ndescription: upstream skill\nlicense: Apache-2.0\n---\n\nBody\n";
        write_file(&external_skill_dir.join("SKILL.md"), external_skill_content)?;
        write_file(
            &external_skill_dir.join("references/commands.md"),
            "Upstream commands\n",
        )?;
        let external_skills = [ExternalSkill::new("herdr", external_skill_dir)?];
        let out = root.join("out");

        render_skills(&root, Provider::Codex, &external_skills, &out)?;

        assert_eq!(
            std::fs::read_to_string(out.join("herdr/SKILL.md"))?,
            external_skill_content,
        );
        assert_eq!(
            std::fs::read_to_string(out.join("herdr/references/commands.md"))?,
            "Upstream commands\n",
        );

        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn external_skill_rejects_a_name_that_escapes_the_output_directory() {
        let result = ExternalSkill::new("../herdr", PathBuf::from("/external/herdr"));

        assert!(result.is_err());
    }

    #[test]
    fn render_skills_rejects_an_external_skill_that_shadows_a_built_in_skill() -> Result<()> {
        let root =
            test_root("render_skills_rejects_an_external_skill_that_shadows_a_built_in_skill")?;
        write_skill_source(&root, "herdr")?;
        let external_skill_dir = root.join("external/herdr");
        write_file(
            &external_skill_dir.join("SKILL.md"),
            "---\nname: herdr\ndescription: upstream skill\n---\n\nBody\n",
        )?;
        let external_skills = [ExternalSkill::new("herdr", external_skill_dir)?];

        let result = render_skills(&root, Provider::Codex, &external_skills, &root.join("out"));

        assert!(result.is_err());
        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn render_skills_rejects_duplicate_external_skill_names() -> Result<()> {
        let root = test_root("render_skills_rejects_duplicate_external_skill_names")?;
        write_skill_source(&root, "built-in")?;
        let first_source = root.join("external/first");
        let second_source = root.join("external/second");
        write_file(
            &first_source.join("SKILL.md"),
            "---\nname: shared\ndescription: first\n---\n\nBody\n",
        )?;
        write_file(
            &second_source.join("SKILL.md"),
            "---\nname: shared\ndescription: second\n---\n\nBody\n",
        )?;
        let external_skills = [
            ExternalSkill::new("shared", first_source)?,
            ExternalSkill::new("shared", second_source)?,
        ];

        let result = render_skills(&root, Provider::Codex, &external_skills, &root.join("out"));

        assert!(result.is_err());
        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn render_skills_rejects_an_external_directory_without_a_skill_file() -> Result<()> {
        let root = test_root("render_skills_rejects_an_external_directory_without_a_skill_file")?;
        write_skill_source(&root, "built-in")?;
        let external_skill_dir = root.join("external/incomplete");
        std::fs::create_dir_all(&external_skill_dir)?;
        let external_skills = [ExternalSkill::new("incomplete", external_skill_dir)?];

        let result = render_skills(&root, Provider::Codex, &external_skills, &root.join("out"));

        assert!(result.is_err());
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

    fn write_skill_rendering(source: &Path, content: &str) -> Result<()> {
        std::fs::create_dir_all(source.join("agents"))?;
        std::fs::write(source.join("agents/skill_rendering.json"), content)?;
        Ok(())
    }

    fn write_file(path: &Path, content: &str) -> Result<()> {
        std::fs::create_dir_all(path.parent().unwrap_or_else(|| Path::new(".")))?;
        std::fs::write(path, content)?;
        Ok(())
    }
}
