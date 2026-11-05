use std::path::Path;

use anyhow::{Context, Result};
use serde_json::{Map, Value, json};

use super::io;

const BUNDLE: &str = "agent-harness-dsh-hooks";

pub(crate) fn sync_config(bundle: &Path, target: &Path) -> Result<()> {
    let bundle = bundle
        .canonicalize()
        .context("failed to resolve dsh bridge bundle")?;
    let mut config: Value = match std::fs::read_to_string(target) {
        Ok(content) => serde_json::from_str(&content).context("invalid dsh profile JSON")?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            eprintln!(
                "Skipping dsh profile {}: run dsh once to initialize it, then reactivate.",
                target.display()
            );
            return Ok(());
        }
        Err(error) => return Err(error.into()),
    };
    let root = config
        .as_object_mut()
        .context("dsh profile must be an object")?;
    object(root, "dependencies")?.insert(
        BUNDLE.to_owned(),
        json!(format!("link:{}", bundle.display())),
    );
    let profile = object(object(root, "dsh")?, "profile")?;
    let bundles = profile
        .entry("bundles")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .context("dsh.profile.bundles must be an array")?;
    if !bundles.iter().any(|value| value.as_str() == Some(BUNDLE)) {
        bundles.push(json!(BUNDLE));
    }
    io::write_json(target, &config)
}

fn object<'a>(parent: &'a mut Map<String, Value>, key: &str) -> Result<&'a mut Map<String, Value>> {
    parent
        .entry(key)
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .with_context(|| format!("dsh profile {key} must be an object"))
}
