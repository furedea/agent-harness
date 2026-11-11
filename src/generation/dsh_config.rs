use anyhow::{Context, Result};
use serde_json::{Map, Value, json};

use crate::{file_op::FileOp, snapshot::ConfigFiles};

const BUNDLE: &str = "agent-harness-dsh-hooks";

/// The updated profile, or nothing when the profile does not exist yet.
///
/// `config.generated` is the bridge bundle directory, already resolved to its real path.
pub(crate) fn sync_config_ops(config: ConfigFiles<'_>) -> Result<Vec<FileOp>> {
    let ConfigFiles {
        generated: bundle,
        target,
        files,
    } = config;
    let Some(content) = files.read_text_if_present(target)? else {
        eprintln!(
            "Skipping dsh profile {}: run dsh once to initialize it, then reactivate.",
            target.display()
        );
        return Ok(Vec::new());
    };
    let mut config: Value = serde_json::from_str(content).context("invalid dsh profile JSON")?;
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
    Ok(vec![FileOp::write_json(target, &config)?])
}

fn object<'a>(parent: &'a mut Map<String, Value>, key: &str) -> Result<&'a mut Map<String, Value>> {
    parent
        .entry(key)
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .with_context(|| format!("dsh profile {key} must be an object"))
}
