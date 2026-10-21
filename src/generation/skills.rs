use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;

use crate::{fs_ops, generation::io, layout::SourceLayout, render::Provider};
use frontmatter::{common_frontmatter_entries, frontmatter_text, split_entries, split_frontmatter};

mod frontmatter;

const SUPPORTED_SKILL_RENDERING_VERSION: u64 = 1;
const CODEX_OPENAI_PATH: &str = "agents/openai.yaml";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SkillMetadata {
    pub(crate) name: String,
    pub(crate) title: String,
    pub(crate) claude_invocation: SkillInvocation,
    pub(crate) codex_invocation: SkillInvocation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SkillInvocation {
    Implicit,
    Explicit,
}

impl SkillInvocation {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Implicit => "implicit",
            Self::Explicit => "explicit",
        }
    }
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
        let source = Some(source)
            .filter(|source| !source.is_empty())
            .ok_or_else(|| "external skill path must not be empty".to_string())?;
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
        ensure!(
            is_valid,
            "external skill name must contain only lowercase ASCII letters, digits, and hyphens: {value}"
        );
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

impl ClaudeRendering {
    fn invocation(&self) -> SkillInvocation {
        match self.frontmatter.get("disable-model-invocation") {
            Some(FrontmatterValue::Bool(true)) => SkillInvocation::Explicit,
            _ => SkillInvocation::Implicit,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct CodexRendering {
    #[serde(default)]
    frontmatter: BTreeMap<String, FrontmatterValue>,
    openai: Option<CodexOpenAi>,
}

impl CodexRendering {
    fn invocation(&self) -> SkillInvocation {
        match &self.openai {
            Some(openai) if !openai.allow_implicit_invocation => SkillInvocation::Explicit,
            _ => SkillInvocation::Implicit,
        }
    }
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

/// Where and for which provider skills are rendered.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SkillTarget<'a> {
    pub(crate) provider: Provider,
    pub(crate) path: &'a Path,
}

impl<'a> SkillTarget<'a> {
    fn join(self, path: &'a Path) -> Self {
        Self {
            provider: self.provider,
            path,
        }
    }
}

pub(crate) fn render_skills(
    source: &Path,
    external_skills: &[ExternalSkill],
    target: SkillTarget<'_>,
) -> Result<()> {
    let skills_dir = SourceLayout::new(source).skills();
    let skill_dirs = sorted_skill_dirs(&skills_dir)?;
    let skill_rendering = read_skill_rendering(source)?;
    validate_skill_rendering_targets(&skill_rendering, &skill_dirs)?;
    validate_external_skills(&skill_dirs, external_skills)?;

    let staging = fs_ops::TemporaryDirectory::create()?;

    for entry in skill_dirs {
        let skill_out = staging.path().join(entry.file_name().unwrap_or_default());
        render_skill_dir(&entry, &skill_rendering, target.join(&skill_out))?;
    }

    for skill in external_skills {
        fs_ops::copy_dir(&skill.source, &staging.path().join(skill.name.as_str()))?;
    }

    fs_ops::copy_dir(staging.path(), target.path)
}

pub(crate) fn built_in_skill_metadata(source: &Path) -> Result<Vec<SkillMetadata>> {
    let skills_dir = SourceLayout::new(source).skills();
    let skill_dirs = sorted_skill_dirs(&skills_dir)?;
    let skill_rendering = read_skill_rendering(source)?;
    validate_skill_rendering_targets(&skill_rendering, &skill_dirs)?;
    let default_rendering = SkillRenderingEntry::default();
    let mut metadata = skill_dirs
        .iter()
        .map(|skill_dir| {
            let name = skill_dir
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default();
            let rendering = skill_rendering
                .skills
                .get(name)
                .unwrap_or(&default_rendering);
            read_skill_metadata(&skill_dir.join("SKILL.md"), rendering)
        })
        .collect::<Result<Vec<_>>>()?;
    metadata.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(metadata)
}

fn sorted_skill_dirs(skills_dir: &Path) -> Result<Vec<PathBuf>> {
    if !skills_dir.exists() {
        return Ok(Vec::new());
    }

    let mut dirs = Vec::new();
    for entry in std::fs::read_dir(skills_dir)
        .with_context(|| format!("failed to read directory {}", skills_dir.display()))?
    {
        let entry =
            entry.with_context(|| format!("failed to read entry in {}", skills_dir.display()))?;
        dirs.extend(skill_dir(&entry)?);
    }
    dirs.sort();
    Ok(dirs)
}

fn skill_dir(entry: &std::fs::DirEntry) -> Result<Option<PathBuf>> {
    let path = entry.path();
    let is_dir = entry
        .file_type()
        .with_context(|| format!("failed to inspect {}", path.display()))?
        .is_dir();
    Ok((is_dir && path.join("SKILL.md").is_file()).then_some(path))
}

fn render_skill_dir(
    source: &Path,
    skill_rendering: &SkillRendering,
    target: SkillTarget<'_>,
) -> Result<()> {
    let source_skill = source.join("SKILL.md");
    let content = std::fs::read_to_string(&source_skill)
        .with_context(|| format!("failed to read {}", source_skill.display()))?;
    let (frontmatter, body) = split_frontmatter(&content)?;
    let mut frontmatter = common_frontmatter_entries(&source_skill, frontmatter)?;
    let skill_name = source
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();

    let empty_rendering = SkillRenderingEntry::default();
    let rendering = skill_rendering
        .skills
        .get(skill_name)
        .unwrap_or(&empty_rendering);

    copy_support_files(source, target.path)?;
    frontmatter.extend(provider_frontmatter_entries(rendering, target.provider)?);
    write_skill(target.path, &frontmatter, body)?;
    write_extra_files(target, rendering)
}

fn read_skill_rendering(source: &Path) -> Result<SkillRendering> {
    let path = SourceLayout::new(source).skill_rendering();
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
        "skill rendering policy references unknown skills: {}",
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
        validate_external_skill(skill, &built_in_names)?;
        ensure!(
            external_names.insert(skill.name.as_str()),
            "duplicate external skill name: {}",
            skill.name.as_str()
        );
    }

    Ok(())
}

fn validate_external_skill(skill: &ExternalSkill, built_in_names: &BTreeSet<&str>) -> Result<()> {
    let has_regular_skill_file = skill
        .source
        .join("SKILL.md")
        .symlink_metadata()
        .is_ok_and(|metadata| metadata.is_file());
    ensure!(
        has_regular_skill_file,
        "external skill directory must contain a regular SKILL.md file: {}",
        skill.source.display()
    );
    ensure!(
        !built_in_names.contains(skill.name.as_str()),
        "external skill shadows a built-in skill: {}",
        skill.name.as_str()
    );
    Ok(())
}

fn read_skill_metadata(path: &Path, rendering: &SkillRenderingEntry) -> Result<SkillMetadata> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let (frontmatter, body) = split_frontmatter(&content)?;
    let entries = split_entries(frontmatter)?;
    let name = frontmatter_text(&entries, "name")?;
    frontmatter_text(&entries, "description")?;
    let title = skill_title(body)?;
    Ok(SkillMetadata {
        name,
        title,
        claude_invocation: rendering.claude.invocation(),
        codex_invocation: rendering.codex.invocation(),
    })
}

fn skill_title(body: &str) -> Result<String> {
    body.lines()
        .find_map(|line| line.strip_prefix("# "))
        .map(|title| title.replace('`', ""))
        .context("SKILL.md body must contain a level-one heading")
}

fn copy_support_files(source: &Path, out: &Path) -> Result<()> {
    std::fs::create_dir_all(out)
        .with_context(|| format!("failed to create directory {}", out.display()))?;

    for entry in std::fs::read_dir(source)
        .with_context(|| format!("failed to read directory {}", source.display()))?
    {
        let entry =
            entry.with_context(|| format!("failed to read entry in {}", source.display()))?;
        copy_support_entry(&entry, out)?;
    }

    Ok(())
}

fn copy_support_entry(entry: &std::fs::DirEntry, out: &Path) -> Result<()> {
    let path = entry.path();
    let name = entry.file_name();
    if name == "SKILL.md" {
        return Ok(());
    }

    let target = out.join(name);
    let file_type = entry
        .file_type()
        .with_context(|| format!("failed to inspect {}", path.display()))?;
    if file_type.is_symlink() {
        return Ok(());
    }
    if file_type.is_dir() {
        fs_ops::copy_dir(&path, &target)?;
    } else if file_type.is_file() {
        fs_ops::copy_file(&path, &target)?;
    }
    Ok(())
}

fn write_skill(out: &Path, frontmatter: &[String], body: &str) -> Result<()> {
    let content = format!("---\n{}\n---\n{body}", frontmatter.join("\n").trim_end());
    io::write_file(&out.join("SKILL.md"), &content)
}

fn provider_frontmatter_entries(
    skill_rendering: &SkillRenderingEntry,
    provider: Provider,
) -> Result<Vec<String>> {
    frontmatter_overrides(skill_rendering, provider)
        .iter()
        .map(|(key, value)| frontmatter_entry(key, value))
        .collect()
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

fn write_extra_files(target: SkillTarget<'_>, skill_rendering: &SkillRenderingEntry) -> Result<()> {
    if let Provider::Codex = target.provider
        && let Some(openai) = &skill_rendering.codex.openai
    {
        io::write_file(
            &target.path.join(CODEX_OPENAI_PATH),
            &codex_openai_content(openai),
        )?;
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
mod tests;
