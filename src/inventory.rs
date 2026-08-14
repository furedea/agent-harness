use std::collections::BTreeSet;
use std::path::Path;

use anyhow::Result;

use crate::generation::{
    hooks::{self, HookMetadata, HookProvider},
    skills::{self, SkillMetadata},
};

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
        format!(
            "Agent Harness inventory\n\n\
             Skills                  {}\n\
             Claude hook events      {}\n\
             Codex hook events       {}\n",
            self.skills.len(),
            self.hook_event_count(HookProvider::Claude),
            self.hook_event_count(HookProvider::Codex),
        )
    }

    pub(crate) fn skills(&self) -> String {
        let mut lines = vec![
            format!("Skills ({})", self.skills.len()),
            String::new(),
            skill_line("NAME", "TITLE", "CLAUDE", "CODEX"),
        ];
        lines.extend(self.skills.iter().map(|skill| {
            skill_line(
                &skill.name,
                &skill.title,
                skill.claude_invocation.as_str(),
                skill.codex_invocation.as_str(),
            )
        }));
        finish_lines(lines)
    }

    pub(crate) fn hooks(&self, provider: Option<HookProvider>) -> String {
        let mut lines = vec!["Hooks".to_owned()];
        for current_provider in [HookProvider::Claude, HookProvider::Codex] {
            if provider.is_some_and(|provider| provider != current_provider) {
                continue;
            }
            let hooks = self
                .hooks
                .iter()
                .filter(|hook| hook.provider == current_provider)
                .collect::<Vec<_>>();
            if hooks.is_empty() {
                continue;
            }
            lines.push(String::new());
            lines.push(current_provider.display_name().to_owned());
            append_hook_lines(&mut lines, &hooks);
        }
        finish_lines(lines)
    }

    fn hook_event_count(&self, provider: HookProvider) -> usize {
        self.hooks
            .iter()
            .filter(|hook| hook.provider == provider)
            .map(|hook| hook.event.as_str())
            .collect::<BTreeSet<_>>()
            .len()
    }
}

fn skill_line(name: &str, title: &str, claude: &str, codex: &str) -> String {
    format!("{name:<18}{title:<44}{claude:<10}{codex}")
}

fn append_hook_lines(lines: &mut Vec<String>, hooks: &[&HookMetadata]) {
    let mut previous_event = None;
    let mut previous_matcher = None;

    for hook in hooks {
        if previous_event != Some(hook.event.as_str()) {
            lines.push(String::new());
            lines.push(hook.event.clone());
            previous_event = Some(&hook.event);
            previous_matcher = None;
        }
        if previous_matcher != Some(hook.matcher.as_str()) {
            lines.push(format!("  matcher: {}", display_matcher(&hook.matcher)));
            previous_matcher = Some(&hook.matcher);
        }
        lines.push(display_hook_command(hook));
    }
}

fn display_matcher(matcher: &str) -> String {
    if matches!(matcher, "" | "*" | ".") {
        return "any".to_owned();
    }

    let matcher = matcher
        .strip_prefix("^(")
        .and_then(|matcher| matcher.strip_suffix(")$"))
        .unwrap_or(matcher);
    let parts = matcher
        .split('|')
        .map(simple_matcher_part)
        .collect::<Option<Vec<_>>>();
    parts.map_or_else(|| matcher.to_owned(), |parts| parts.join(" | "))
}

fn simple_matcher_part(part: &str) -> Option<String> {
    let part = part.trim_start_matches('^').trim_end_matches('$');
    let part = part.replace("\\.", ".");
    (!part.is_empty()
        && part
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "_-.".contains(character)))
    .then_some(part)
}

fn display_hook_command(hook: &HookMetadata) -> String {
    let command = shorten_hook_command(&hook.command);
    match hook.condition.as_deref() {
        Some(condition) => format!("    {command:<29}when: {}", display_condition(condition)),
        None => format!("    {command}"),
    }
}

fn shorten_hook_command(command: &str) -> String {
    let parts = command
        .split_whitespace()
        .map(shorten_hook_path)
        .collect::<Vec<_>>();
    match parts.as_slice() {
        [adapter, target] if adapter == "adapt_shell_command.sh" => {
            format!("{adapter} -> {target}")
        }
        _ => parts.join(" "),
    }
}

fn shorten_hook_path(part: &str) -> String {
    part.strip_prefix("$HOME/.claude/hooks/")
        .or_else(|| part.strip_prefix("$HOME/.codex/hooks/"))
        .unwrap_or(part)
        .to_owned()
}

fn display_condition(condition: &str) -> String {
    let mut patterns = Vec::new();
    for entry in condition.split('|') {
        let Some((_tool, pattern)) = entry.split_once('(') else {
            return condition.to_owned();
        };
        let Some(pattern) = pattern.strip_suffix(')') else {
            return condition.to_owned();
        };
        if !patterns.contains(&pattern) {
            patterns.push(pattern);
        }
    }
    patterns.join(", ")
}

fn finish_lines(lines: Vec<String>) -> String {
    format!("{}\n", lines.join("\n"))
}
