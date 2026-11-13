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

pub(crate) fn read_to_string(path: impl AsRef<Path>) -> Result<String> {
    std::fs::read_to_string(path)
}

pub(crate) fn read(path: impl AsRef<Path>) -> Result<Vec<u8>> {
    std::fs::read(path)
}

pub(crate) fn exists(path: impl AsRef<Path>) -> bool {
    path.as_ref().exists()
}

pub(crate) fn is_file(path: impl AsRef<Path>) -> bool {
    path.as_ref().is_file()
}

pub(crate) fn is_dir(path: impl AsRef<Path>) -> bool {
    path.as_ref().is_dir()
}

pub(crate) fn metadata(path: impl AsRef<Path>) -> Result<std::fs::Metadata> {
    std::fs::metadata(path)
}

pub(crate) fn set_permissions(
    path: impl AsRef<Path>,
    permissions: std::fs::Permissions,
) -> Result<()> {
    std::fs::set_permissions(path, permissions)
}

pub(crate) fn remove_file(path: impl AsRef<Path>) -> Result<()> {
    std::fs::remove_file(path)
}

pub(crate) fn remove_dir(path: impl AsRef<Path>) -> Result<()> {
    std::fs::remove_dir(path)
}

pub(crate) fn rename(from: impl AsRef<Path>, to: impl AsRef<Path>) -> Result<()> {
    std::fs::rename(from, to)
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
        write_if_missing(&path, default_content(relative, providers))?;
    }
    layout
        .required_directories_for(providers)
        .into_iter()
        .try_for_each(create_dir_all)
}

fn write_if_missing(path: &Path, content: String) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        create_dir_all(parent)?;
    }
    write(path, content)
}

fn default_content(relative: &str, providers: &[crate::provider::spec::ProviderSpec]) -> String {
    match relative {
        "manifest.json" => r#"{"version":1,"runtime_commands":[]}"#.to_owned(),
        "AGENTS.md" => "agent instructions\n".to_owned(),
        "hooks.json" => default_hooks(providers),
        "command_permissions.json"
        | "hooks/rules/secret_path_policy.json"
        | "hooks/rules/allowed_commands.json"
        | "hooks/rules/forbidden_commands.json" => r#"{"version":1,"rules":[]}"#.to_owned(),
        "skill_rendering.json" => r#"{"version":1}"#.to_owned(),
        "claude/settings.json" | "hooks/rules/secret_commit_policy.json" => "{}".to_owned(),
        _ => String::new(),
    }
}

fn default_section(format: crate::provider::spec::HookFormat) -> serde_json::Value {
    use crate::provider::spec::HookFormat::{BareEvents, SettingsSection, WrappedEvents};
    match format {
        SettingsSection => serde_json::json!({}),
        WrappedEvents | BareEvents => serde_json::json!({"hooks": {}}),
    }
}

fn default_hooks(providers: &[crate::provider::spec::ProviderSpec]) -> String {
    use crate::provider::spec::HookSection;
    let sections: serde_json::Map<String, serde_json::Value> = providers
        .iter()
        .filter(|spec| spec.hook.section == HookSection::Required)
        .map(|spec| (spec.key.to_owned(), default_section(spec.hook.format)))
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
