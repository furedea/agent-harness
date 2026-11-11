use crate::provider::spec::{
    CommandRuleSpec, HookFormat, HookSection, PROVIDERS, ProviderId, ProviderSpec,
};
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
    #[serde(flatten)]
    sections: BTreeMap<String, Value>,
}

impl HookConfig {
    /// The provider's section, or `None` when it is absent or null.
    fn section(&self, spec: &ProviderSpec) -> Option<&Value> {
        self.sections
            .get(spec.key)
            .filter(|section| !section.is_null())
    }

    /// The provider's section, or the empty one of its format when an optional one is absent.
    fn into_section(mut self, spec: &ProviderSpec) -> Value {
        self.sections
            .remove(spec.key)
            .filter(|section| !section.is_null())
            .unwrap_or_else(|| empty_section(spec.hook.format))
    }
}

fn empty_section(format: HookFormat) -> Value {
    match format {
        HookFormat::SettingsSection => serde_json::json!({}),
        HookFormat::WrappedEvents | HookFormat::BareEvents => serde_json::json!({ "hooks": {} }),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HookMetadata {
    pub(crate) provider: ProviderId,
    pub(crate) event: String,
    pub(crate) matcher: String,
    pub(crate) condition: Option<String>,
    pub(crate) command: String,
}

/// Writes the hook configuration a provider reads, in the shape that provider expects.
pub(crate) fn write_hooks_for_runtime(
    context: GenerationContext<'_>,
    path: &Path,
    spec: &ProviderSpec,
) -> Result<()> {
    io::write_json(path, &hooks_for_runtime(context, spec)?)
}

fn hooks_for_runtime(context: GenerationContext<'_>, spec: &ProviderSpec) -> Result<Value> {
    let mut hooks = read_hooks(context.source)?.into_section(spec);
    for bundle in context.external_hooks {
        merge_bundle(bundle, &mut hooks, spec)?;
    }
    relocate_commands(&mut hooks, context.runtime_root);
    if spec.command_rule == Some(CommandRuleSpec::InHookRegistration) {
        hooks
            .as_object_mut()
            .with_context(|| format!("hook section for {} is not a JSON object", spec.key))?
            .insert(
                "permissions".to_owned(),
                command_permissions::devin_permissions(context.source)?,
            );
    }
    Ok(hooks)
}

pub(crate) fn claude_hooks_for_runtime(context: GenerationContext<'_>) -> Result<Value> {
    hooks_for_runtime(context, ProviderId::Claude.spec())
}

fn merge_bundle(bundle: &ExternalHookBundle, hooks: &mut Value, spec: &ProviderSpec) -> Result<()> {
    match spec.hook.format {
        HookFormat::SettingsSection => bundle.merge_claude_hooks(hooks),
        HookFormat::WrappedEvents => bundle.merge_codex_hooks(hooks),
        HookFormat::BareEvents => bundle.merge_provider_hooks(hooks, spec),
    }
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
    let sections = PROVIDERS
        .iter()
        .filter_map(|spec| Some((spec, config.section(spec)?)));
    let mut metadata = Vec::new();
    for (spec, section) in sections {
        metadata.extend(section_hook_metadata(spec, section)?);
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

/// A settings section is its event map itself; every other section nests it under `hooks`.
fn section_hook_metadata(spec: &ProviderSpec, section: &Value) -> Result<Vec<HookMetadata>> {
    if spec.hook.format == HookFormat::SettingsSection {
        return hook_metadata(spec.id, section);
    }
    let hooks = section
        .get("hooks")
        .with_context(|| format!("hook config {} section must contain hooks", spec.key))?;
    hook_metadata(spec.id, hooks)
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
    let content =
        io::read_file(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let config: HookConfig = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    validate_hooks(&config)?;
    Ok(config)
}

fn validate_hooks(config: &HookConfig) -> Result<()> {
    if config.version != 1 {
        bail!("unsupported hook config version: {}", config.version);
    }
    if let Some(key) = config
        .sections
        .keys()
        .find(|key| !PROVIDERS.iter().any(|spec| spec.key == key.as_str()))
    {
        bail!("hook config contains unknown provider key: {key}");
    }
    let invalid = PROVIDERS
        .iter()
        .find(|spec| !section_is_valid(spec, config.section(spec)));
    if let Some(spec) = invalid {
        bail!("hook config {} section must be a JSON object", spec.key);
    }
    Ok(())
}

fn section_is_valid(spec: &ProviderSpec, section: Option<&Value>) -> bool {
    match (spec.hook.section, section) {
        (_, Some(section)) => section.is_object(),
        (HookSection::Optional, None) => true,
        (HookSection::Required, None) => false,
    }
}

fn hook_metadata(provider: ProviderId, value: &Value) -> Result<Vec<HookMetadata>> {
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
    provider: ProviderId,
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
    provider: ProviderId,
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
