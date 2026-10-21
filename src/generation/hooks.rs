use std::cmp::Ordering;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::{
    generation::{GenerationContext, command_permissions, external_hooks::ExternalHookBundle, io},
    layout::SourceLayout,
    runtime_root::RuntimeRoot,
};

#[derive(Debug, Deserialize)]
struct HookConfig {
    version: u64,
    claude: Value,
    codex: Value,
    devin: Option<Value>,
    hermes: Option<Value>,
    pi: Option<Value>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, clap::ValueEnum)]
pub(crate) enum HookProvider {
    Claude,
    Codex,
    Devin,
    Hermes,
    Pi,
}

impl HookProvider {
    pub(crate) fn display_name(self) -> &'static str {
        match self {
            Self::Claude => "Claude",
            Self::Codex => "Codex",
            Self::Devin => "Devin",
            Self::Hermes => "Hermes",
            Self::Pi => "Pi",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HookMetadata {
    pub(crate) provider: HookProvider,
    pub(crate) event: String,
    pub(crate) matcher: String,
    pub(crate) condition: Option<String>,
    pub(crate) command: String,
}

pub(crate) fn write_claude_hooks(
    source: &Path,
    path: &Path,
    external_hooks: &[ExternalHookBundle],
) -> Result<()> {
    let context = GenerationContext::home(source, external_hooks);
    io::write_json(path, &claude_hooks_for_runtime(context)?)
}

pub(crate) fn write_codex_hooks(
    source: &Path,
    path: &Path,
    external_hooks: &[ExternalHookBundle],
) -> Result<()> {
    write_codex_hooks_for_runtime(GenerationContext::home(source, external_hooks), path)
}

pub(crate) fn write_codex_hooks_for_runtime(
    context: GenerationContext<'_>,
    path: &Path,
) -> Result<()> {
    let mut hooks = read_hooks(context.source)?.codex;
    for bundle in context.external_hooks {
        bundle.merge_codex_hooks(&mut hooks)?;
    }
    relocate_commands(&mut hooks, context.runtime_root);
    io::write_json(path, &hooks)
}

pub(crate) fn write_devin_hooks(
    source: &Path,
    path: &Path,
    external_hooks: &[ExternalHookBundle],
) -> Result<()> {
    write_devin_hooks_for_runtime(GenerationContext::home(source, external_hooks), path)
}

pub(crate) fn write_devin_hooks_for_runtime(
    context: GenerationContext<'_>,
    path: &Path,
) -> Result<()> {
    io::write_json(path, &devin_hooks_for_runtime(context)?)
}

pub(crate) fn devin_hooks_for_runtime(context: GenerationContext<'_>) -> Result<Value> {
    let mut config = read_hooks(context.source)?
        .devin
        .unwrap_or_else(|| serde_json::json!({ "hooks": {} }));
    for bundle in context.external_hooks {
        bundle.merge_devin_hooks(&mut config)?;
    }
    relocate_commands(&mut config, context.runtime_root);
    let Some(root) = config.as_object_mut() else {
        bail!("hook config devin section must be a JSON object");
    };
    root.insert(
        "permissions".to_string(),
        command_permissions::devin_permissions(context.source)?,
    );
    Ok(config)
}

pub(crate) fn write_hermes_hooks(
    source: &Path,
    path: &Path,
    external_hooks: &[ExternalHookBundle],
) -> Result<()> {
    write_hermes_hooks_for_runtime(GenerationContext::home(source, external_hooks), path)
}

pub(crate) fn write_hermes_hooks_for_runtime(
    context: GenerationContext<'_>,
    path: &Path,
) -> Result<()> {
    io::write_json(path, &hermes_hooks_for_runtime(context)?)
}

pub(crate) fn hermes_hooks_for_runtime(context: GenerationContext<'_>) -> Result<Value> {
    let mut hooks = read_hooks(context.source)?
        .hermes
        .unwrap_or_else(|| serde_json::json!({ "hooks": {} }));
    for bundle in context.external_hooks {
        bundle.merge_hermes_hooks(&mut hooks)?;
    }
    relocate_commands(&mut hooks, context.runtime_root);
    Ok(hooks)
}

pub(crate) fn write_pi_hooks(
    source: &Path,
    path: &Path,
    external_hooks: &[ExternalHookBundle],
) -> Result<()> {
    write_pi_hooks_for_runtime(GenerationContext::home(source, external_hooks), path)
}

pub(crate) fn write_pi_hooks_for_runtime(
    context: GenerationContext<'_>,
    path: &Path,
) -> Result<()> {
    io::write_json(path, &pi_hooks_for_runtime(context)?)
}

pub(crate) fn pi_hooks_for_runtime(context: GenerationContext<'_>) -> Result<Value> {
    let mut hooks = read_hooks(context.source)?
        .pi
        .unwrap_or_else(|| serde_json::json!({ "hooks": {} }));
    for bundle in context.external_hooks {
        bundle.merge_pi_hooks(&mut hooks)?;
    }
    relocate_commands(&mut hooks, context.runtime_root);
    Ok(hooks)
}

pub(crate) fn claude_hooks_for_runtime(context: GenerationContext<'_>) -> Result<Value> {
    let mut hooks = read_hooks(context.source)?.claude;
    for bundle in context.external_hooks {
        bundle.merge_claude_hooks(&mut hooks)?;
    }
    relocate_commands(&mut hooks, context.runtime_root);
    Ok(hooks)
}

fn relocate_commands(value: &mut Value, runtime_root: &RuntimeRoot) {
    match value {
        Value::Array(items) => relocate_each(items.iter_mut(), runtime_root),
        Value::Object(object) => relocate_object_commands(object, runtime_root),
        _ => {}
    }
}

fn relocate_object_commands(object: &mut Map<String, Value>, runtime_root: &RuntimeRoot) {
    if let Some(Value::String(command)) = object.get_mut("command") {
        *command = runtime_root.relocate_command(command);
    }
    relocate_each(object.values_mut(), runtime_root);
}

fn relocate_each<'a>(items: impl Iterator<Item = &'a mut Value>, runtime_root: &RuntimeRoot) {
    for item in items {
        relocate_commands(item, runtime_root);
    }
}

pub(crate) fn built_in_hook_metadata(source: &Path) -> Result<Vec<HookMetadata>> {
    let config = read_hooks(source)?;
    let codex = config
        .codex
        .get("hooks")
        .context("hook config codex section must contain hooks")?;
    let mut metadata = hook_metadata(HookProvider::Claude, &config.claude)?;
    metadata.extend(hook_metadata(HookProvider::Codex, codex)?);
    if let Some(devin) = &config.devin {
        let devin = devin
            .get("hooks")
            .context("hook config devin section must contain hooks")?;
        metadata.extend(hook_metadata(HookProvider::Devin, devin)?);
    }
    let optional_sections = [
        (HookProvider::Hermes, &config.hermes),
        (HookProvider::Pi, &config.pi),
    ]
    .into_iter()
    .filter_map(|(provider, section)| Some((provider, section.as_ref()?)));
    for (provider, section) in optional_sections {
        metadata.extend(optional_section_hook_metadata(provider, section)?);
    }
    metadata.sort_by(compare_hook_metadata);
    Ok(metadata)
}

fn compare_hook_metadata(left: &HookMetadata, right: &HookMetadata) -> Ordering {
    left.provider
        .cmp(&right.provider)
        .then_with(|| hook_event_rank(&left.event).cmp(&hook_event_rank(&right.event)))
        .then_with(|| left.event.cmp(&right.event))
}

fn optional_section_hook_metadata(
    provider: HookProvider,
    section: &Value,
) -> Result<Vec<HookMetadata>> {
    let section_name = provider.display_name().to_lowercase();
    let hooks = section
        .get("hooks")
        .with_context(|| format!("hook config {section_name} section must contain hooks"))?;
    hook_metadata(provider, hooks)
}

fn hook_event_rank(event: &str) -> usize {
    match event {
        "SessionStart" => 0,
        "UserPromptSubmit" => 1,
        "PreToolUse" => 2,
        "PostToolUse" => 3,
        "PermissionDenied" => 4,
        "Notification" => 5,
        "SubagentStop" | "PermissionRequest" => 6,
        "Stop" => 7,
        "PreCompact" => 8,
        "PostCompaction" => 9,
        "SessionEnd" => 10,
        _ => usize::MAX,
    }
}

fn read_hooks(source: &Path) -> Result<HookConfig> {
    let path = SourceLayout::new(source).hook_config();
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let config: HookConfig = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    validate_hooks(&config)?;
    Ok(config)
}

fn validate_hooks(config: &HookConfig) -> Result<()> {
    if config.version != 1 {
        bail!("unsupported hook config version: {}", config.version);
    }
    if !config.claude.is_object() {
        bail!("hook config claude section must be a JSON object");
    }
    if !config.codex.is_object() {
        bail!("hook config codex section must be a JSON object");
    }
    let invalid_optional_section = [
        ("devin", &config.devin),
        ("hermes", &config.hermes),
        ("pi", &config.pi),
    ]
    .into_iter()
    .find(|(_name, section)| section.as_ref().is_some_and(is_not_object));
    if let Some((name, _section)) = invalid_optional_section {
        bail!("hook config {name} section must be a JSON object");
    }
    Ok(())
}

fn is_not_object(value: &Value) -> bool {
    !value.is_object()
}

fn hook_metadata(provider: HookProvider, value: &Value) -> Result<Vec<HookMetadata>> {
    let events = value
        .as_object()
        .context("hook provider section must be a JSON object")?;
    let mut metadata = Vec::new();

    for (event, groups) in events {
        metadata.extend(event_hook_metadata(provider, event, groups)?);
    }
    Ok(metadata)
}

fn event_hook_metadata(
    provider: HookProvider,
    event: &str,
    groups: &Value,
) -> Result<Vec<HookMetadata>> {
    let groups = groups
        .as_array()
        .with_context(|| format!("hook event {event} must be a JSON array"))?;
    let mut metadata = Vec::new();
    for group in groups {
        metadata.extend(group_hook_metadata(provider, event, group)?);
    }
    Ok(metadata)
}

fn group_hook_metadata(
    provider: HookProvider,
    event: &str,
    group: &Value,
) -> Result<Vec<HookMetadata>> {
    let group = group
        .as_object()
        .with_context(|| format!("hook group for {event} must be a JSON object"))?;
    let matcher = group
        .get("matcher")
        .and_then(Value::as_str)
        .unwrap_or("*")
        .to_owned();
    let hooks = group
        .get("hooks")
        .and_then(Value::as_array)
        .with_context(|| format!("hook group for {event} must contain a hooks array"))?;

    let location = HookLocation {
        provider,
        event,
        matcher: &matcher,
    };
    hooks
        .iter()
        .map(|hook| hook_metadata_entry(&location, hook))
        .collect()
}

struct HookLocation<'a> {
    provider: HookProvider,
    event: &'a str,
    matcher: &'a str,
}

fn hook_metadata_entry(location: &HookLocation<'_>, hook: &Value) -> Result<HookMetadata> {
    let event = location.event;
    let hook = hook
        .as_object()
        .with_context(|| format!("hook entry for {event} must be a JSON object"))?;
    let command = hook
        .get("command")
        .and_then(Value::as_str)
        .with_context(|| format!("hook entry for {event} must contain a command"))?;
    let condition = hook.get("if").and_then(Value::as_str).map(str::to_owned);
    Ok(HookMetadata {
        provider: location.provider,
        event: event.to_owned(),
        matcher: location.matcher.to_owned(),
        condition,
        command: command.to_owned(),
    })
}

#[cfg(test)]
mod tests;
