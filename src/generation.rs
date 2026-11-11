#![forbid(clippy::disallowed_methods, clippy::disallowed_types)]

use crate::{runtime_root::RuntimeRoot, snapshot::Tree};

pub(crate) mod bridges;
pub(crate) mod claude_config;
mod claude_permissions;
pub(crate) mod codex_config;
pub(crate) mod codex_managed_state;
pub(crate) mod command_permissions;
pub(crate) mod devin_config;
pub(crate) mod external_hooks;
pub(crate) mod hermes_config;
pub(crate) mod hook_bundle;
pub(crate) mod hooks;
mod json_documents;
pub(crate) mod permission_rule;
pub(crate) mod pi_config;
pub(crate) mod pi_sandbox;
pub(crate) mod protection;
pub(crate) mod secret_path_policy;
pub(crate) mod skills;

/// Inputs shared by generators that render runtime-specific artifacts.
#[derive(Clone, Copy, Debug)]
pub(crate) struct GenerationContext<'a> {
    pub(crate) dsh_profile: &'a str,
    pub(crate) source: Tree<'a>,
    pub(crate) external_hooks: &'a [external_hooks::ExternalHookBundle],
    pub(crate) runtime_root: &'a RuntimeRoot,
}

impl<'a> GenerationContext<'a> {
    pub(crate) fn home(
        source: Tree<'a>,
        external_hooks: &'a [external_hooks::ExternalHookBundle],
    ) -> Self {
        static HOME: RuntimeRoot = RuntimeRoot::Home;
        Self {
            dsh_profile: "web",
            source,
            external_hooks,
            runtime_root: &HOME,
        }
    }
}

pub(crate) mod dsh_config;
