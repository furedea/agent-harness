use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{Context, Result, ensure};

use crate::{file_op::FileOp, render::Provider, source_model::Source};
use frontmatter::{common_frontmatter_entries, frontmatter_text, split_entries, split_frontmatter};

mod frontmatter;
mod rendering;

use rendering::{
    DEFAULT_RENDERING, SkillRendering, SkillRenderingEntry, extra_file_op, parse_skill_rendering,
    provider_frontmatter_entries, validate_skill_rendering_targets,
};

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
    /// The file whose presence makes the directory a skill, for the loader to look at.
    pub(crate) fn skill_file(&self) -> PathBuf {
        self.source.join("SKILL.md")
    }

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

#[derive(Clone, Copy, Debug)]
pub(crate) struct SkillTarget<'a> {
    pub(crate) provider: Provider,
    pub(crate) path: &'a Path,
}

/// A file or directory of a skill that is copied beside its rendered `SKILL.md`.
///
/// Only the location is kept: the content is copied by the executor, never loaded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SupportEntry {
    File { name: OsString, path: PathBuf },
    Directory { name: OsString, path: PathBuf },
}

/// What the loader found in one built-in skill directory.
#[derive(Debug)]
pub(crate) struct SkillFiles {
    pub(crate) directory: PathBuf,
    pub(crate) skill_text: String,
    pub(crate) support: Vec<SupportEntry>,
}

/// A built-in skill, parsed once.
#[derive(Debug)]
struct SourceSkill {
    name: String,
    frontmatter: Vec<String>,
    body: String,
    metadata: SkillMetadata,
    support: Vec<SupportEntry>,
}

impl SourceSkill {
    fn parse(files: SkillFiles, rendering: &SkillRendering) -> Result<Self> {
        let skill_file = files.directory.join("SKILL.md");
        let name = directory_name(&files.directory);
        let invalid = || format!("invalid skill file {}", skill_file.display());
        let (frontmatter, body) = split_frontmatter(&files.skill_text).with_context(invalid)?;
        let entries = split_entries(frontmatter).with_context(invalid)?;
        let frontmatter_name = frontmatter_text(&entries, "name").with_context(invalid)?;
        frontmatter_text(&entries, "description").with_context(invalid)?;
        let title = skill_title(body).with_context(invalid)?;
        let entry = rendering.skills.get(&name).unwrap_or(&DEFAULT_RENDERING);
        Ok(Self {
            frontmatter: common_frontmatter_entries(&skill_file, frontmatter)?,
            body: body.to_owned(),
            metadata: SkillMetadata {
                name: frontmatter_name,
                title,
                claude_invocation: entry.claude.invocation(),
                codex_invocation: entry.codex.invocation(),
            },
            support: files.support,
            name,
        })
    }

    fn render(
        &self,
        rendering: &SkillRenderingEntry,
        target: SkillTarget<'_>,
    ) -> Result<Vec<FileOp>> {
        let SkillTarget {
            provider,
            path: out,
        } = target;
        let mut frontmatter = self.frontmatter.clone();
        let mut operations = self.support_file_ops(out);
        frontmatter.extend(provider_frontmatter_entries(rendering, provider)?);
        operations.push(skill_op(out, &frontmatter, &self.body));
        operations.extend(extra_file_op(provider, out, rendering));
        Ok(operations)
    }

    fn support_file_ops(&self, out: &Path) -> Vec<FileOp> {
        self.support
            .iter()
            .map(|entry| support_entry_op(entry, out))
            .collect()
    }
}

fn directory_name(directory: &Path) -> String {
    directory
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_owned()
}

/// The built-in skills of a source and their rendering policy, parsed and validated.
#[derive(Debug, Default)]
pub(crate) struct SourceSkills {
    rendering: SkillRendering,
    skills: Vec<SourceSkill>,
}

impl SourceSkills {
    /// Parses the rendering policy read from `rendering` and every skill directory in `files`.
    ///
    /// # Errors
    ///
    /// Returns an error naming the file that is invalid.
    pub(crate) fn parse(rendering: Option<(&Path, &str)>, files: Vec<SkillFiles>) -> Result<Self> {
        let rendering = rendering
            .map(|(path, text)| parse_skill_rendering(path, text))
            .transpose()?
            .unwrap_or_default();
        let names: BTreeSet<String> = files
            .iter()
            .map(|files| directory_name(&files.directory))
            .collect();
        validate_skill_rendering_targets(&rendering, &names)?;
        let skills = files
            .into_iter()
            .map(|files| SourceSkill::parse(files, &rendering))
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { rendering, skills })
    }

    pub(crate) fn metadata(&self) -> Vec<SkillMetadata> {
        let mut metadata: Vec<_> = self
            .skills
            .iter()
            .map(|skill| skill.metadata.clone())
            .collect();
        metadata.sort_by(|left, right| left.name.cmp(&right.name));
        metadata
    }

    /// Checks external skills against the built-in ones.
    ///
    /// `has_skill_file` says, per external skill, whether its directory holds a regular `SKILL.md`.
    ///
    /// # Errors
    ///
    /// Returns an error when an external skill has no skill file, shadows a built-in one, or
    /// repeats another external skill's name.
    pub(crate) fn validate_external(
        &self,
        external_skills: &[ExternalSkill],
        has_skill_file: &[bool],
    ) -> Result<()> {
        let built_in_names: BTreeSet<&str> = self
            .skills
            .iter()
            .map(|skill| skill.name.as_str())
            .collect();
        validate_external_skills(external_skills, has_skill_file, &built_in_names)?;
        reject_duplicate_external_names(external_skills)
    }

    fn rendering_of(&self, skill: &SourceSkill) -> &SkillRenderingEntry {
        self.rendering
            .skills
            .get(&skill.name)
            .unwrap_or(&DEFAULT_RENDERING)
    }
}

/// The operation that replaces the skills directory with every rendered skill.
///
/// # Errors
///
/// Returns an error when a frontmatter override of the rendering policy is invalid.
pub(crate) fn render_skills(source: &Source, target: SkillTarget<'_>) -> Result<FileOp> {
    let mut contents = Vec::new();
    for skill in &source.skills.skills {
        let skill_out = target.path.join(&skill.name);
        contents.extend(skill.render(
            source.skills.rendering_of(skill),
            SkillTarget {
                provider: target.provider,
                path: &skill_out,
            },
        )?);
    }

    contents.extend(source.external_skills.iter().map(|skill| FileOp::CopyTree {
        source: skill.source.clone(),
        target: target.path.join(skill.name.as_str()),
    }));

    Ok(FileOp::ReplaceTree {
        target: target.path.to_path_buf(),
        contents,
    })
}

fn validate_external_skills(
    external_skills: &[ExternalSkill],
    has_skill_file: &[bool],
    built_in_names: &BTreeSet<&str>,
) -> Result<()> {
    for (skill, has_file) in external_skills.iter().zip(has_skill_file) {
        validate_external_skill(skill, *has_file, built_in_names)?;
    }
    Ok(())
}

fn reject_duplicate_external_names(external_skills: &[ExternalSkill]) -> Result<()> {
    let mut names = BTreeSet::new();
    let duplicate = external_skills
        .iter()
        .find(|skill| !names.insert(skill.name.as_str()));
    ensure!(
        duplicate.is_none(),
        "duplicate external skill name: {}",
        duplicate.map_or("", |skill| skill.name.as_str())
    );
    Ok(())
}

fn validate_external_skill(
    skill: &ExternalSkill,
    has_skill_file: bool,
    built_in_names: &BTreeSet<&str>,
) -> Result<()> {
    ensure!(
        has_skill_file,
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

fn skill_title(body: &str) -> Result<String> {
    body.lines()
        .find_map(|line| line.strip_prefix("# "))
        .map(|title| title.replace('`', ""))
        .context("SKILL.md body must contain a level-one heading")
}

/// Copies a support file or directory of a skill beside its rendered `SKILL.md`.
fn support_entry_op(entry: &SupportEntry, out: &Path) -> FileOp {
    match entry {
        SupportEntry::Directory { name, path } => FileOp::CopyTree {
            source: path.clone(),
            target: out.join(name),
        },
        SupportEntry::File { name, path } => FileOp::CopyFile {
            source: path.clone(),
            target: out.join(name),
        },
    }
}

fn skill_op(out: &Path, frontmatter: &[String], body: &str) -> FileOp {
    let content = format!("---\n{}\n---\n{body}", frontmatter.join("\n").trim_end());
    FileOp::write(out.join("SKILL.md"), content)
}

#[cfg(test)]
mod tests;
