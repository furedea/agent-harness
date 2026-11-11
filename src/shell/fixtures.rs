//! Test-only filesystem helpers for computation modules, which may not call std's write APIs.
#![allow(
    clippy::disallowed_methods,
    reason = "test fixtures build their own temporary trees"
)]

use std::io::Result;
use std::path::Path;

pub(crate) fn write(path: impl AsRef<Path>, content: impl AsRef<[u8]>) -> Result<()> {
    std::fs::write(path, content)
}

pub(crate) fn create_dir_all(path: impl AsRef<Path>) -> Result<()> {
    std::fs::create_dir_all(path)
}

pub(crate) fn create_dir(path: impl AsRef<Path>) -> Result<()> {
    std::fs::create_dir(path)
}

pub(crate) fn remove_dir_all(path: impl AsRef<Path>) -> Result<()> {
    std::fs::remove_dir_all(path)
}

#[cfg(unix)]
pub(crate) fn symlink(original: impl AsRef<Path>, link: impl AsRef<Path>) -> Result<()> {
    std::os::unix::fs::symlink(original, link)
}

/// Loads the source tree at `root` with every provider, first adding a minimal valid version of
/// each required file the test did not write, so a test states only what it is about.
pub(crate) fn load_source(root: &Path) -> anyhow::Result<crate::source_model::Source> {
    load_source_with(
        root,
        &super::LoadInputs::new(crate::provider::spec::PROVIDERS),
    )
}

/// Like [`load_source`], with the external inputs and providers of `inputs`.
pub(crate) fn load_source_with(
    root: &Path,
    inputs: &super::LoadInputs<'_>,
) -> anyhow::Result<crate::source_model::Source> {
    complete_source(root, inputs.providers)?;
    super::load_source(root, inputs)
}

fn complete_source(root: &Path, providers: &[crate::provider::spec::ProviderSpec]) -> Result<()> {
    let layout = crate::layout::SourceLayout::new(root);
    for (relative, path) in layout.required_files() {
        if path.exists() {
            continue;
        }
        if let Some(parent) = path.parent() {
            create_dir_all(parent)?;
        }
        write(&path, default_content(relative, providers))?;
    }
    layout
        .required_directories_for(providers)
        .into_iter()
        .try_for_each(create_dir_all)
}

fn default_content(relative: &str, providers: &[crate::provider::spec::ProviderSpec]) -> String {
    match relative {
        "manifest.json" => r#"{"version":1,"runtime_commands":[]}"#.to_owned(),
        "AGENTS.md" => "agent instructions\n".to_owned(),
        "hooks.json" => default_hooks(providers),
        "command_permissions.json" | "hooks/rules/secret_path_policy.json" => {
            r#"{"version":1,"rules":[]}"#.to_owned()
        }
        "hooks/rules/allowed_commands.json" | "hooks/rules/forbidden_commands.json" => {
            r#"{"version":1,"rules":[]}"#.to_owned()
        }
        "skill_rendering.json" => r#"{"version":1}"#.to_owned(),
        "claude/settings.json" | "hooks/rules/secret_commit_policy.json" => "{}".to_owned(),
        _ => String::new(),
    }
}

fn default_hooks(providers: &[crate::provider::spec::ProviderSpec]) -> String {
    use crate::provider::spec::{HookFormat, HookSection};
    let sections: serde_json::Map<String, serde_json::Value> = providers
        .iter()
        .filter(|spec| spec.hook.section == HookSection::Required)
        .map(|spec| {
            let section = match spec.hook.format {
                HookFormat::SettingsSection => serde_json::json!({}),
                HookFormat::WrappedEvents | HookFormat::BareEvents => {
                    serde_json::json!({"hooks": {}})
                }
            };
            (spec.key.to_owned(), section)
        })
        .collect();
    let mut config = serde_json::json!({"version": 1});
    if let Some(object) = config.as_object_mut() {
        object.extend(sections);
    }
    config.to_string()
}

/// The regular files under `root`, sorted, for tests that compare directory contents.
pub(crate) fn regular_files(root: &Path) -> anyhow::Result<Vec<std::path::PathBuf>> {
    super::load::regular_files(root)
}
