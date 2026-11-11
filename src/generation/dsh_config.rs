use std::path::Path;

use anyhow::Result;
use serde_json::json;

use crate::config_sync::codec::{ConfigDocument, Json};

const BUNDLE: &str = "agent-harness-dsh-hooks";

/// The registration of the bridge bundle in a dsh profile: a link to the bundle directory and its
/// name in the profile's bundle list.
///
/// `bundle` is the bridge directory, already resolved to its real path.
pub(crate) fn bundle_registration(bundle: &Path) -> Result<ConfigDocument> {
    Json::from_value(json!({
        "dependencies": {BUNDLE: format!("link:{}", bundle.display())},
        "dsh": {"profile": {"bundles": [BUNDLE]}},
    }))
    .map(ConfigDocument::Json)
}
