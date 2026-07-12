use anyhow::{Result, bail};
use serde_json::{Map, Value, json};

const CLAUDE_COMMAND: &str = "bash \"$HOME/.claude/hooks/herdr-agent-state.sh\" session";
const CODEX_COMMAND: &str = "bash \"$HOME/.codex/herdr-agent-state.sh\" session";

pub(crate) fn is_enabled() -> bool {
    std::env::var("HERDR_ENV").is_ok_and(|value| value == "1")
}

pub(crate) fn claude_hooks(mut hooks: Value) -> Result<Value> {
    if !is_enabled() {
        return Ok(hooks);
    }

    prepend_group(
        &mut hooks,
        "SessionStart",
        json!({
            "matcher": "*",
            "hooks": [
                {
                    "command": CLAUDE_COMMAND,
                    "timeout": 10,
                    "type": "command"
                }
            ]
        }),
    )?;
    Ok(hooks)
}

pub(crate) fn codex_hooks(mut hooks: Value) -> Result<Value> {
    if !is_enabled() {
        return Ok(hooks);
    }

    let root = object_mut(&mut hooks, "Codex hooks root")?;
    let events = object_entry(root, "hooks")?;
    prepend_group(
        events,
        "SessionStart",
        json!({
            "hooks": [
                {
                    "command": CODEX_COMMAND,
                    "timeout": 10,
                    "type": "command"
                }
            ]
        }),
    )?;
    Ok(hooks)
}

fn prepend_group(root: &mut Value, event: &str, group: Value) -> Result<()> {
    let object = object_mut(root, "hook root")?;
    let entry = object
        .entry(event.to_string())
        .or_insert_with(|| Value::Array(Vec::new()));
    let Some(groups) = entry.as_array_mut() else {
        bail!("hook event {event} must be a JSON array");
    };

    if !groups
        .iter()
        .any(|existing| group_command(existing) == group_command(&group))
    {
        groups.insert(0, group);
    }

    Ok(())
}

fn group_command(group: &Value) -> Option<&str> {
    group["hooks"][0]["command"].as_str()
}

fn object_entry<'a>(root: &'a mut Map<String, Value>, key: &str) -> Result<&'a mut Value> {
    Ok(root
        .entry(key.to_string())
        .or_insert_with(|| Value::Object(Map::new())))
}

fn object_mut<'a>(value: &'a mut Value, name: &str) -> Result<&'a mut Map<String, Value>> {
    match value {
        Value::Object(object) => Ok(object),
        _ => bail!("{name} must be a JSON object"),
    }
}
