use crate::provider::spec::{PROVIDERS, ProviderId, ProviderSpec};
use std::cmp::Ordering;
use std::collections::BTreeMap;
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
    #[serde(flatten)]
    providers: BTreeMap<String, Value>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, clap::ValueEnum)]
pub(crate) enum HookProvider {
    Claude,
    Codex,
    Devin,
    Hermes,
    Pi,
    Dsh,
}

impl HookProvider {
    pub(crate) fn display_name(self) -> &'static str {
        match self {
            Self::Claude => "Claude",
            Self::Codex => "Codex",
            Self::Devin => ProviderId::Devin.spec().display_name,
            Self::Hermes => ProviderId::Hermes.spec().display_name,
            Self::Pi => ProviderId::Pi.spec().display_name,
            Self::Dsh => ProviderId::Dsh.spec().display_name,
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

pub(crate) fn write_provider_hooks_for_runtime(
    context: GenerationContext<'_>,
    path: &Path,
    spec: &ProviderSpec,
) -> Result<()> {
    io::write_json(path, &provider_hooks_for_runtime(context, spec)?)
}

fn provider_hooks_for_runtime(
    context: GenerationContext<'_>,
    spec: &ProviderSpec,
) -> Result<Value> {
    let mut hooks = read_hooks(context.source)?
        .providers
        .remove(spec.key)
        .filter(|section| !section.is_null())
        .unwrap_or_else(|| serde_json::json!({ "hooks": {} }));
    for bundle in context.external_hooks {
        bundle.merge_provider_hooks(&mut hooks, spec)?;
    }
    relocate_commands(&mut hooks, context.runtime_root);
    if spec.permissions {
        hooks
            .as_object_mut()
            .expect("validated hook section")
            .insert(
                "permissions".to_owned(),
                command_permissions::devin_permissions(context.source)?,
            );
    }
    Ok(hooks)
}

pub(crate) fn hook_provider(id: ProviderId) -> HookProvider {
    match id {
        ProviderId::Devin => HookProvider::Devin,
        ProviderId::Hermes => HookProvider::Hermes,
        ProviderId::Pi => HookProvider::Pi,
        ProviderId::Dsh => HookProvider::Dsh,
    }
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
    let sections = PROVIDERS.iter().filter_map(|spec| {
        let section = config
            .providers
            .get(spec.key)
            .filter(|section| !section.is_null())?;
        Some((hook_provider(spec.id), section))
    });
    for (provider, section) in sections {
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
    let invalid = PROVIDERS.iter().find(|spec| {
        config
            .providers
            .get(spec.key)
            .is_some_and(|section| !section.is_null() && is_not_object(section))
    });
    if let Some(spec) = invalid {
        bail!("hook config {} section must be a JSON object", spec.key);
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

    hooks
        .iter()
        .map(|hook| {
            let hook = hook_entry(event, hook)?;
            Ok(HookMetadata {
                provider,
                event: event.to_owned(),
                matcher: matcher.clone(),
                condition: hook.get("if").and_then(Value::as_str).map(str::to_owned),
                command: hook_command(event, hook)?.to_owned(),
            })
        })
        .collect()
}

fn hook_entry<'a>(event: &str, hook: &'a Value) -> Result<&'a Map<String, Value>> {
    hook.as_object()
        .with_context(|| format!("hook entry for {event} must be a JSON object"))
}

fn hook_command<'a>(event: &str, hook: &'a Map<String, Value>) -> Result<&'a str> {
    hook.get("command")
        .and_then(Value::as_str)
        .with_context(|| format!("hook entry for {event} must contain a command"))
}

#[cfg(test)]
mod tests;
