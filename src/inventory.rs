use std::collections::BTreeSet;
use std::path::Path;

use crate::provider::spec::{PROVIDERS, ProviderId};
use anyhow::Result;

use crate::generation::{
    hooks::{self, HookMetadata},
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
        let mut lines = vec![
            "Agent Harness inventory".to_owned(),
            String::new(),
            format!("{:<24}{}", "Skills", self.skills.len()),
        ];
        lines.extend(inventory_providers().map(|provider| self.hook_summary(provider)));
        finish_lines(&lines)
    }

    pub(crate) fn skills(&self) -> String {
        let mut lines = vec![
            format!("Skills ({})", self.skills.len()),
            String::new(),
            SKILL_HEADER.to_string(),
        ];
        lines.extend(
            self.skills
                .iter()
                .map(|skill| SkillRow::from(skill).to_string()),
        );
        finish_lines(&lines)
    }

    pub(crate) fn hooks(&self, provider: Option<ProviderId>) -> String {
        let mut lines = vec!["Hooks".to_owned()];
        let providers = inventory_providers();
        lines.extend(
            providers
                .filter(|current| provider.is_none_or(|provider| provider == *current))
                .flat_map(|current| provider_hook_lines(&self.hooks, current)),
        );
        finish_lines(&lines)
    }

    fn hook_summary(&self, provider: ProviderId) -> String {
        format!(
            "{:<24}{}",
            format!("{} hook events", provider.spec().display_name),
            self.hook_event_count(provider)
        )
    }

    fn hook_event_count(&self, provider: ProviderId) -> usize {
        self.hooks
            .iter()
            .filter(|hook| hook.provider == provider)
            .map(|hook| hook.event.as_str())
            .collect::<BTreeSet<_>>()
            .len()
    }
}

/// Providers in identifier order, independent of the registry's installation order.
fn inventory_providers() -> impl Iterator<Item = ProviderId> {
    let mut providers: Vec<_> = PROVIDERS.iter().map(|spec| spec.id).collect();
    providers.sort();
    providers.into_iter()
}

const SKILL_HEADER: SkillRow<'static> = SkillRow {
    name: "NAME",
    title: "TITLE",
    claude: "CLAUDE",
    codex: "CODEX",
};

/// One row of the skill inventory table.
struct SkillRow<'a> {
    name: &'a str,
    title: &'a str,
    claude: &'a str,
    codex: &'a str,
}

impl<'a> From<&'a SkillMetadata> for SkillRow<'a> {
    fn from(skill: &'a SkillMetadata) -> Self {
        Self {
            name: &skill.name,
            title: &skill.title,
            claude: skill.claude_invocation.as_str(),
            codex: skill.codex_invocation.as_str(),
        }
    }
}

impl std::fmt::Display for SkillRow<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self {
            name,
            title,
            claude,
            codex,
        } = self;
        write!(formatter, "{name:<18}{title:<44}{claude:<10}{codex}")
    }
}

fn provider_hook_lines(hooks: &[HookMetadata], provider: ProviderId) -> Vec<String> {
    let hooks = hooks
        .iter()
        .filter(|hook| hook.provider == provider)
        .collect::<Vec<_>>();
    if hooks.is_empty() {
        return Vec::new();
    }
    let mut lines = vec![String::new(), provider.spec().display_name.to_owned()];
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
    shorten_hook_path_for(part, PROVIDERS)
}

pub(crate) fn shorten_hook_path_for(
    part: &str,
    providers: &[crate::provider::spec::ProviderSpec],
) -> String {
    providers
        .iter()
        .map(|spec| spec.hook.scripts_dir)
        .find_map(|dir| part.strip_prefix(&format!("$HOME/{dir}/")))
        .unwrap_or(part)
        .to_owned()
}

fn display_condition(condition: &str) -> String {
    let Some(mut patterns) = condition
        .split('|')
        .map(condition_pattern)
        .collect::<Option<Vec<_>>>()
    else {
        return condition.to_owned();
    };
    let mut seen = std::collections::HashSet::new();
    patterns.retain(|pattern| seen.insert(*pattern));
    patterns.join(", ")
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
    use crate::generation::skills::SkillInvocation;

    fn hook(provider: ProviderId, event: &str, matcher: &str) -> HookMetadata {
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
                    hook(ProviderId::Claude, "PreToolUse", "Bash"),
                    "Bash(git *)|Bash(gh *)|Bash(git *)",
                ),
                conditional(hook(ProviderId::Claude, "PreToolUse", "Bash"), "malformed"),
                conditional(
                    hook(ProviderId::Claude, "PreToolUse", "^(Edit|Write)$"),
                    "Edit(x",
                ),
                hook(ProviderId::Claude, "Stop", ""),
                hook(ProviderId::Pi, "tool_call", "*"),
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
            inventory().hooks(Some(ProviderId::Pi)),
            "Hooks\n\nPi\n\ntool_call\n  matcher: any\n    tool_call_*.sh\n"
        );
        assert_eq!(inventory().hooks(Some(ProviderId::Codex)), "Hooks\n");
    }

    #[test]
    fn skills_aligns_rows_under_the_header() {
        let inventory = Inventory {
            skills: vec![SkillMetadata {
                name: "git-workflow".to_owned(),
                title: "Git Workflow".to_owned(),
                claude_invocation: SkillInvocation::Implicit,
                codex_invocation: SkillInvocation::Explicit,
            }],
            hooks: Vec::new(),
        };
        assert_eq!(
            inventory.skills(),
            "Skills (1)\n\n\
             NAME              TITLE                                       CLAUDE    CODEX\n\
             git-workflow      Git Workflow                                implicit  explicit\n"
        );
    }

    #[test]
    fn inventory_lists_every_registry_provider() {
        let inventory = Inventory {
            skills: Vec::new(),
            hooks: PROVIDERS
                .iter()
                .map(|spec| hook(spec.id, "event", "*"))
                .collect(),
        };
        for spec in PROVIDERS {
            assert!(
                inventory
                    .summary()
                    .contains(&format!("{} hook events", spec.display_name))
            );
            assert!(
                inventory
                    .hooks(None)
                    .contains(&format!("\n{}\n", spec.display_name))
            );
        }
    }

    fn shortened_adapter_command(provider_dir: &str) -> String {
        shorten_hook_command(&format!(
            "$HOME/{provider_dir}/hooks/hook_adapter.py shell forbidden"
        ))
    }

    #[test]
    fn hook_commands_drop_the_provider_hooks_directory() {
        let expected = "hook_adapter.py shell forbidden";
        for spec in PROVIDERS {
            assert_eq!(shortened_adapter_command(spec.home_dir), expected);
        }
    }
}
