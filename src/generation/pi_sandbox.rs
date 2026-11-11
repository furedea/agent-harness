use std::path::Path;

use anyhow::{Context, Result};
use serde_json::{Value, json};

use super::{GenerationContext, claude_config, json_documents::read_json};
use crate::{file_op::FileOp, layout::SourceLayout};

pub(crate) fn policy_op(context: GenerationContext<'_>, out: &Path) -> Result<FileOp> {
    let base = read_json(
        context.source.files(),
        &SourceLayout::new(context.source.root()).claude_settings(),
    )?;
    let settings = claude_config::build_settings(context, base)?;
    let sandbox = settings.get("sandbox").context("missing sandbox")?;
    let network = sandbox.get("network").unwrap_or(&Value::Null);
    let filesystem = sandbox.get("filesystem").context("missing filesystem")?;
    let mut sockets = strings(network, "allowUnixSockets")?;
    sockets.push(json!("/nix/var/nix/daemon-socket/socket"));
    let mut deny_read = strings(filesystem, "denyRead")?;
    deny_read.push(json!("~/.pi/agent/auth.json"));
    let policy = json!({
        "network": {
            "allowedDomains": strings(network, "allowedDomains")?,
            "deniedDomains": strings(network, "deniedDomains")?,
            "allowUnixSockets": sockets
        },
        "filesystem": {
            "denyRead": deny_read,
            "allowWrite": strings(filesystem, "allowWrite")?,
            "denyWrite": strings(filesystem, "denyWrite")?
        }
    });
    FileOp::write_json(
        out.join(".pi/agent/extensions/sandbox_policy.json"),
        &policy,
    )
}

fn strings(object: &Value, key: &str) -> Result<Vec<Value>> {
    let Some(value) = object.get(key) else {
        return Ok(Vec::new());
    };
    let values = value
        .as_array()
        .with_context(|| format!("{key} must be an array"))?;
    anyhow::ensure!(
        values
            .iter()
            .all(|value| value.as_str().is_some_and(|s| !s.is_empty())),
        "{key} entries must be nonempty strings"
    );
    Ok(values.clone())
}
