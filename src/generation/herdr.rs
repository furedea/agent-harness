use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use serde_json::Value;

const CLAUDE_SCRIPT: &str = ".claude/hooks/herdr-agent-state.sh";
const CODEX_SCRIPT: &str = ".codex/herdr-agent-state.sh";

pub(crate) struct TemporaryIntegration {
    root: PathBuf,
}

impl TemporaryIntegration {
    pub(crate) fn generate(herdr_bin: &Path) -> Result<Self> {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let root = std::env::temp_dir().join(format!(
            "agent-harness-herdr-{}-{nanos}",
            std::process::id()
        ));
        generate(herdr_bin, &root)?;
        Ok(Self { root })
    }

    pub(crate) fn path(&self) -> &Path {
        &self.root
    }
}

impl Drop for TemporaryIntegration {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

pub(crate) fn generate(herdr_bin: &Path, output: &Path) -> Result<()> {
    std::fs::create_dir_all(output)
        .with_context(|| format!("failed to create {}", output.display()))?;
    for provider_dir in [".claude", ".codex"] {
        std::fs::create_dir_all(output.join(provider_dir))
            .with_context(|| format!("failed to create Herdr integration {provider_dir}"))?;
    }
    for provider in ["claude", "codex"] {
        let result = Command::new(herdr_bin)
            .args(["integration", "install", provider])
            .env("HOME", output)
            .output()
            .with_context(|| format!("failed to execute {}", herdr_bin.display()))?;
        if !result.status.success() {
            bail!(
                "Herdr {provider} integration install failed: {}",
                String::from_utf8_lossy(&result.stderr).trim()
            );
        }
    }
    for relative in [CLAUDE_SCRIPT, CODEX_SCRIPT] {
        if !output.join(relative).is_file() {
            bail!("Herdr integration did not generate {relative}");
        }
    }
    Ok(())
}

pub(crate) fn merge_claude_hooks(mut hooks: Value, integration: Option<&Path>) -> Result<Value> {
    let Some(integration) = integration else {
        return Ok(hooks);
    };
    let generated = read_json(&integration.join(".claude/settings.json"))?;
    merge_events(&mut hooks, &generated["hooks"], integration, CLAUDE_SCRIPT)?;
    Ok(hooks)
}

pub(crate) fn merge_codex_hooks(mut hooks: Value, integration: Option<&Path>) -> Result<Value> {
    let Some(integration) = integration else {
        return Ok(hooks);
    };
    let generated = read_json(&integration.join(".codex/hooks.json"))?;
    merge_events(
        &mut hooks["hooks"],
        &generated["hooks"],
        integration,
        CODEX_SCRIPT,
    )?;
    Ok(hooks)
}

pub(crate) fn copy_scripts(integration: &Path, output: &Path) -> Result<()> {
    crate::fs_ops::copy_file(&integration.join(CODEX_SCRIPT), &output.join(CODEX_SCRIPT))?;
    crate::fs_ops::copy_file(
        &integration.join(CLAUDE_SCRIPT),
        &output.join(CLAUDE_SCRIPT),
    )
}

fn merge_events(
    target: &mut Value,
    generated: &Value,
    integration: &Path,
    script: &str,
) -> Result<()> {
    let target = target
        .as_object_mut()
        .context("hook root must be a JSON object")?;
    let generated = generated
        .as_object()
        .context("generated Herdr hooks must be a JSON object")?;
    let generated_script = integration.join(script).to_string_lossy().into_owned();
    let final_script = format!("$HOME/{script}");

    for (event, groups) in generated {
        let groups = groups
            .as_array()
            .with_context(|| format!("Herdr hook event {event} must be an array"))?;
        let target_groups = target
            .entry(event.clone())
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
            .with_context(|| format!("hook event {event} must be an array"))?;
        for group in groups {
            let mut group = group.clone();
            replace_command_path(&mut group, &generated_script, &final_script);
            if !target_groups.contains(&group) {
                target_groups.insert(0, group);
            }
        }
    }
    Ok(())
}

fn replace_command_path(value: &mut Value, source: &str, target: &str) {
    match value {
        Value::Array(items) => {
            for item in items {
                replace_command_path(item, source, target);
            }
        }
        Value::Object(object) => {
            if let Some(Value::String(command)) = object.get_mut("command") {
                *command = normalize_command(command, source, target);
            }
            for item in object.values_mut() {
                replace_command_path(item, source, target);
            }
        }
        _ => {}
    }
}

fn normalize_command(command: &str, source: &str, target: &str) -> String {
    let quoted_target = format!("\"{target}\"");
    command
        .replace(&format!("'{source}'"), &quoted_target)
        .replace(&format!("\"{source}\""), &quoted_target)
        .replace(source, target)
}

fn read_json(path: &Path) -> Result<Value> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&content).with_context(|| format!("failed to parse {}", path.display()))
}
