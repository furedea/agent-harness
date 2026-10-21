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
             Codex hook events       {}\n\
             Devin hook events       {}\n\
             Hermes hook events      {}\n\
             Pi hook events          {}\n",
            self.skills.len(),
            self.hook_event_count(HookProvider::Claude),
            self.hook_event_count(HookProvider::Codex),
            self.hook_event_count(HookProvider::Devin),
            self.hook_event_count(HookProvider::Hermes),
            self.hook_event_count(HookProvider::Pi),
        )
    }

    pub(crate) fn skills(&self) -> String {
        let mut lines = vec![
            format!("Skills ({})", self.skills.len()),
            String::new(),
            skill_line(["NAME", "TITLE", "CLAUDE", "CODEX"]),
        ];
        lines.extend(self.skills.iter().map(skill_row));
        finish_lines(&lines)
    }

    pub(crate) fn hooks(&self, provider: Option<HookProvider>) -> String {
        let mut lines = vec!["Hooks".to_owned()];
        let providers = [
            HookProvider::Claude,
            HookProvider::Codex,
            HookProvider::Devin,
            HookProvider::Hermes,
            HookProvider::Pi,
        ];
        lines.extend(
            providers
                .into_iter()
                .filter(|current| provider.is_none_or(|provider| provider == *current))
                .flat_map(|current| provider_hook_lines(&self.hooks, current)),
        );
        finish_lines(&lines)
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

fn skill_row(skill: &SkillMetadata) -> String {
    skill_line([
        &skill.name,
        &skill.title,
        skill.claude_invocation.as_str(),
        skill.codex_invocation.as_str(),
    ])
}

fn skill_line([name, title, claude, codex]: [&str; 4]) -> String {
    format!("{name:<18}{title:<44}{claude:<10}{codex}")
}

fn provider_hook_lines(hooks: &[HookMetadata], provider: HookProvider) -> Vec<String> {
    let hooks = hooks
        .iter()
        .filter(|hook| hook.provider == provider)
        .collect::<Vec<_>>();
    if hooks.is_empty() {
        return Vec::new();
    }
    let mut lines = vec![String::new(), provider.display_name().to_owned()];
    append_hook_lines(&mut lines, &hooks);
    lines
}

fn append_hook_lines(lines: &mut Vec<String>, hooks: &[&HookMetadata]) {
    let previous_hooks = std::iter::once(None).chain(hooks.iter().copied().map(Some));
    for (previous, hook) in previous_hooks.zip(hooks) {
        lines.extend(hook_lines(previous, hook));
    }
}

/// Lists a hook under event and matcher headings, repeating a heading only when it changes.
fn hook_lines(previous: Option<&HookMetadata>, hook: &HookMetadata) -> Vec<String> {
    let is_new_event = previous.is_none_or(|previous| previous.event != hook.event);
    let is_new_matcher =
        is_new_event || previous.is_some_and(|previous| previous.matcher != hook.matcher);
    let mut lines = Vec::new();
    if is_new_event {
        lines.push(String::new());
        lines.push(hook.event.clone());
    }
    if is_new_matcher {
        lines.push(format!("  matcher: {}", display_matcher(&hook.matcher)));
    }
    lines.push(display_hook_command(hook));
    lines
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
        .or_else(|| part.strip_prefix("$HOME/.devin/hooks/"))
        .or_else(|| part.strip_prefix("$HOME/.hermes/hooks/"))
        .or_else(|| part.strip_prefix("$HOME/.pi/hooks/"))
        .unwrap_or(part)
        .to_owned()
}

fn display_condition(condition: &str) -> String {
    let Some(patterns) = condition
        .split('|')
        .map(condition_pattern)
        .collect::<Option<Vec<_>>>()
    else {
        return condition.to_owned();
    };
    patterns
        .iter()
        .enumerate()
        .filter(|(index, pattern)| !patterns[..*index].contains(pattern))
        .map(|(_, pattern)| *pattern)
        .collect::<Vec<_>>()
        .join(", ")
}

fn condition_pattern(entry: &str) -> Option<&str> {
    let (_tool, pattern) = entry.split_once('(')?;
    pattern.strip_suffix(')')
}

fn finish_lines(lines: &[String]) -> String {
    format!("{}\n", lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hook(provider: HookProvider, event: &str, matcher: &str) -> HookMetadata {
        HookMetadata {
            provider,
            event: event.to_owned(),
            matcher: matcher.to_owned(),
            condition: None,
            command: format!("$HOME/.claude/hooks/{event}_{matcher}.sh"),
        }
    }

    fn conditional(hook: HookMetadata, condition: &str) -> HookMetadata {
        HookMetadata {
            condition: Some(condition.to_owned()),
            ..hook
        }
    }

    fn inventory() -> Inventory {
        Inventory {
            skills: Vec::new(),
            hooks: vec![
                conditional(
                    hook(HookProvider::Claude, "PreToolUse", "Bash"),
                    "Bash(git *)|Bash(gh *)|Bash(git *)",
                ),
                conditional(
                    hook(HookProvider::Claude, "PreToolUse", "Bash"),
                    "malformed",
                ),
                conditional(
                    hook(HookProvider::Claude, "PreToolUse", "^(Edit|Write)$"),
                    "Edit(x",
                ),
                hook(HookProvider::Claude, "Stop", ""),
                hook(HookProvider::Pi, "tool_call", "*"),
            ],
        }
    }

    #[test]
    fn hooks_groups_by_provider_event_and_matcher() {
        assert_eq!(
            inventory().hooks(None),
            "Hooks\n\nClaude\n\nPreToolUse\n  matcher: Bash\n    \
             PreToolUse_Bash.sh           when: git *, gh *\n    \
             PreToolUse_Bash.sh           when: malformed\n  matcher: Edit | Write\n    \
             PreToolUse_^(Edit|Write)$.sh when: Edit(x\n\nStop\n  matcher: any\n    \
             Stop_.sh\n\nPi\n\ntool_call\n  matcher: any\n    tool_call_*.sh\n"
        );
    }

    #[test]
    fn hooks_shows_only_the_requested_provider() {
        assert_eq!(
            inventory().hooks(Some(HookProvider::Pi)),
            "Hooks\n\nPi\n\ntool_call\n  matcher: any\n    tool_call_*.sh\n"
        );
        assert_eq!(inventory().hooks(Some(HookProvider::Codex)), "Hooks\n");
    }
}
