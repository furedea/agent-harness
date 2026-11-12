//! The per-skill rendering policy of a source: frontmatter overrides and the Codex invocation file.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use super::SkillInvocation;
use crate::{file_op::FileOp, render::Provider};

const SUPPORTED_SKILL_RENDERING_VERSION: u64 = 1;
const CODEX_OPENAI_PATH: &str = "agents/openai.yaml";

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SkillRendering {
    pub(super) version: u64,
    #[serde(default)]
    pub(super) skills: BTreeMap<String, SkillRenderingEntry>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SkillRenderingEntry {
    #[serde(default)]
    pub(super) claude: ClaudeRendering,
    #[serde(default)]
    pub(super) codex: CodexRendering,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ClaudeRendering {
    #[serde(default)]
    pub(super) frontmatter: BTreeMap<String, FrontmatterValue>,
}

impl ClaudeRendering {
    pub(super) fn invocation(&self) -> SkillInvocation {
        match self.frontmatter.get("disable-model-invocation") {
            Some(FrontmatterValue::Bool(true)) => SkillInvocation::Explicit,
            _ => SkillInvocation::Implicit,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CodexRendering {
    #[serde(default)]
    pub(super) frontmatter: BTreeMap<String, FrontmatterValue>,
    pub(super) openai: Option<CodexOpenAi>,
}

impl CodexRendering {
    pub(super) fn invocation(&self) -> SkillInvocation {
        match &self.openai {
            Some(openai) if !openai.allow_implicit_invocation => SkillInvocation::Explicit,
            _ => SkillInvocation::Implicit,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CodexOpenAi {
    pub(super) allow_implicit_invocation: bool,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(super) enum FrontmatterValue {
    Bool(bool),
    String(String),
}

pub(super) static DEFAULT_RENDERING: SkillRenderingEntry = SkillRenderingEntry {
    claude: ClaudeRendering {
        frontmatter: BTreeMap::new(),
    },
    codex: CodexRendering {
        frontmatter: BTreeMap::new(),
        openai: None,
    },
};

pub(super) fn parse_skill_rendering(path: &Path, text: &str) -> Result<SkillRendering> {
    let rendering: SkillRendering = serde_json::from_str(text)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    if rendering.version != SUPPORTED_SKILL_RENDERING_VERSION {
        bail!("unsupported skill rendering version: {}", rendering.version);
    }
    Ok(rendering)
}

pub(super) fn validate_skill_rendering_targets(
    skill_rendering: &SkillRendering,
    skill_names: &BTreeSet<String>,
) -> Result<()> {
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

pub(super) fn provider_frontmatter_entries(
    skill_rendering: &SkillRenderingEntry,
    provider: Provider,
) -> Result<Vec<String>> {
    frontmatter_overrides(skill_rendering, provider)
        .iter()
        .map(|(key, value)| frontmatter_entry(key, value))
        .collect()
}

pub(super) fn frontmatter_overrides(
    skill_rendering: &SkillRenderingEntry,
    provider: Provider,
) -> &BTreeMap<String, FrontmatterValue> {
    match provider {
        Provider::Claude => &skill_rendering.claude.frontmatter,
        Provider::Codex => &skill_rendering.codex.frontmatter,
    }
}

pub(super) fn frontmatter_entry(key: &str, value: &FrontmatterValue) -> Result<String> {
    validate_frontmatter_key(key)?;
    let rendered = match value {
        FrontmatterValue::Bool(value) => value.to_string(),
        FrontmatterValue::String(value) => serde_json::to_string(value)?,
    };
    Ok(format!("{key}: {rendered}"))
}

pub(super) fn validate_frontmatter_key(key: &str) -> Result<()> {
    if key.is_empty()
        || !key
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
    {
        bail!("skill rendering frontmatter key is invalid: {key}");
    }
    Ok(())
}

pub(super) fn extra_file_op(
    provider: Provider,
    out: &Path,
    skill_rendering: &SkillRenderingEntry,
) -> Option<FileOp> {
    let Provider::Codex = provider else {
        return None;
    };
    let openai = skill_rendering.codex.openai.as_ref()?;
    Some(FileOp::write(
        out.join(CODEX_OPENAI_PATH),
        codex_openai_content(openai),
    ))
}

pub(super) fn codex_openai_content(openai: &CodexOpenAi) -> String {
    format!(
        "policy:\n  allow_implicit_invocation: {}\n",
        openai.allow_implicit_invocation
    )
}
