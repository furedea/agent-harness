use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::fs_ops;

/// Sidecar next to the Codex config recording the `shell_environment_policy.set` keys last
/// written, so a key dropped from the source is removed while keys Codex or the user added stay.
pub(super) const MANAGED_STATE_FILE: &str = ".agent-harness-managed.json";
/// Managed before the sidecar existed, so it is removed from the source's absence alone.
const LEGACY_MANAGED_ENV_KEYS: &[&str] = &["GH_CONFIG_DIR"];

#[derive(Serialize, Deserialize)]
pub(super) struct ManagedState {
    #[serde(default)]
    pub(super) shell_environment_policy_set: BTreeSet<String>,
}

impl ManagedState {
    fn legacy() -> Self {
        Self {
            shell_environment_policy_set: LEGACY_MANAGED_ENV_KEYS
                .iter()
                .map(ToString::to_string)
                .collect(),
        }
    }
}

pub(super) fn managed_state_path(target_path: &Path) -> PathBuf {
    target_path.with_file_name(MANAGED_STATE_FILE)
}

// A missing or unreadable sidecar must not block the sync; it only forgoes removals, apart from
// the keys managed before the sidecar existed.
pub(super) fn read_managed_state(path: &Path) -> ManagedState {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_else(ManagedState::legacy)
}

pub(super) fn write_managed_state(path: &Path, state: &ManagedState) -> Result<()> {
    let content = serde_json::to_string_pretty(state)? + "\n";
    fs_ops::write_file_atomically(path, content.as_bytes())
}
