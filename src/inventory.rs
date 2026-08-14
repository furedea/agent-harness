use std::path::Path;

use anyhow::Result;

use crate::generation::{
    hooks::{self, HookMetadata, HookProvider},
    skills::{self, SkillMetadata},
};

const MANAGED_SOURCES: [(&str, &str); 6] = [
    ("agent instructions", "agents/AGENTS.md"),
    ("command policy", "agents/command_policy.json"),
    ("hook rules", "agents/hooks/rules"),
    ("claude settings", "claude/settings.base.json"),
    ("codex config", "codex/config.toml"),
    ("claude statusline", "claude/statusline"),
];

#[derive(Debug)]
pub(crate) struct Inventory {
    skills: Vec<SkillMetadata>,
    hooks: Vec<HookMetadata>,
}

impl Inventory {
    pub(crate) fn load(source: &Path) -> Result<Self> {
        Ok(Self {
            skills: skills::built_in_skill_metadata(source)?,
            hooks: hooks::built_in_hook_metadata(source)?,
        })
    }

    pub(crate) fn summary(&self) -> String {
        let mut lines = vec![
            "Managed components".to_owned(),
            format!("- skills: {}", self.skills.len()),
            format!("- claude hooks: {}", self.hook_count(HookProvider::Claude)),
            format!("- codex hooks: {}", self.hook_count(HookProvider::Codex)),
        ];
        lines.extend(
            MANAGED_SOURCES
                .iter()
                .map(|(name, path)| format!("- {name}: {path}")),
        );
        finish_lines(lines)
    }

    pub(crate) fn skills(&self) -> String {
        let mut lines = vec!["Skills".to_owned()];
        lines.extend(
            self.skills
                .iter()
                .map(|skill| format!("- {}: {}", skill.name, skill.description)),
        );
        finish_lines(lines)
    }

    pub(crate) fn hooks(&self, provider: Option<HookProvider>) -> String {
        let mut lines = vec!["Hooks".to_owned()];
        lines.extend(
            self.hooks
                .iter()
                .filter(|hook| provider.is_none_or(|provider| hook.provider == provider))
                .map(hook_line),
        );
        finish_lines(lines)
    }

    fn hook_count(&self, provider: HookProvider) -> usize {
        self.hooks
            .iter()
            .filter(|hook| hook.provider == provider)
            .count()
    }
}

fn hook_line(hook: &HookMetadata) -> String {
    let condition = hook
        .condition
        .as_deref()
        .map(|condition| format!(" / if {condition}"))
        .unwrap_or_default();
    format!(
        "- {} / {} / {}{}: {}",
        hook.provider.as_str(),
        hook.event,
        hook.matcher,
        condition,
        hook.command,
    )
}

fn finish_lines(lines: Vec<String>) -> String {
    format!("{}\n", lines.join("\n"))
}
