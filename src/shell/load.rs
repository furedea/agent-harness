//! The loader: reads the disk once and hands computation a typed [`Source`].
//!
//! Computation never reads files, so everything it decides on is read and parsed here first. Every
//! document is parsed and validated at this boundary and an error names the file it came from; a
//! directory that is only copied is recorded as a location and a list of files, never read.
//!
//! A tree is walked following a symlinked directory one hop (a profile may share a `hooks`
//! directory), never a symlink met inside that one, so a link cycle cannot loop and a copy never
//! reaches outside its directory.
#![expect(
    clippy::disallowed_methods,
    reason = "this module is the read side of the shell; computation only sees the Source"
)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};

use crate::{
    config_sync::{
        codec::ConfigDocument,
        installed::{Installed, InstalledConfigs, InstalledText, parse_generated},
    },
    generation::{
        command_permissions::{CommandPermissions, RegexPolicy},
        external_hooks::ExternalHookBundle,
        hooks::HookConfig,
        secret_path_policy::SecretPathPolicy,
        skills::{ExternalSkill, SkillFiles, SourceSkills, SupportEntry},
    },
    layout::SourceLayout,
    manifest::Manifest,
    provider::spec::{ConfigSpec, ProviderSpec, RenderedConfig},
    source_model::{Source, SourceDir},
};

/// What a run adds to the source tree: the providers it renders and the external inputs.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LoadInputs<'a> {
    pub(crate) providers: &'a [ProviderSpec],
    pub(crate) hook_bundles: &'a [ExternalHookBundle],
    pub(crate) skills: &'a [ExternalSkill],
}

impl<'a> LoadInputs<'a> {
    pub(crate) fn new(providers: &'a [ProviderSpec]) -> Self {
        Self {
            providers,
            hook_bundles: &[],
            skills: &[],
        }
    }
}

/// Reads and parses the source tree at `root` and the external inputs.
///
/// # Errors
///
/// Returns an error naming the file that cannot be read or is invalid.
pub(crate) fn load_source(root: &Path, inputs: &LoadInputs<'_>) -> Result<Source> {
    let layout = SourceLayout::new(root);
    let skills = load_skills(&layout)?;
    let external_skills = inputs.skills.to_vec();
    skills.validate_external(&external_skills, &external_skill_files(&external_skills))?;
    let directories = load_directories(&layout, inputs.providers)?;
    require_directories(&layout, inputs.providers, &directories)?;
    validate_allowed_commands(&layout)?;
    Ok(Source {
        root: root.to_path_buf(),
        manifest: read_manifest(root)?,
        hooks: HookConfig::parse(&layout.hook_config(), &read_text(&layout.hook_config())?)?,
        command_permissions: load_command_permissions(&layout)?,
        forbidden_commands: load_regex_policy(&layout.forbidden_command_rules())?,
        secret_paths: load_secret_paths(&layout)?,
        skills,
        claude_settings: read_json(&layout.claude_settings())?,
        codex_config: read_toml(&layout.codex_config())?,
        provider_configs: load_provider_configs(root, inputs.providers)?,
        directories,
        hook_bundles: load_bundles(inputs)?,
        external_skills,
    })
}

/// The manifest of the source tree at `root`.
///
/// # Errors
///
/// Returns an error when the manifest cannot be read or is not supported.
pub(crate) fn read_manifest(root: &Path) -> Result<Manifest> {
    let path = SourceLayout::new(root).manifest();
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read source manifest {}", path.display()))?;
    Manifest::parse(&path, &text)
}

/// Whether every file a source tree must hold is in `root`.
pub(crate) fn has_required_files(root: &Path) -> bool {
    missing_required_files(root).is_empty()
}

/// Checks that `root` is a complete source tree: its files, the directories `providers` need, and
/// a supported manifest.
///
/// # Errors
///
/// Returns an error naming what is missing or invalid.
pub(crate) fn validate_source_layout(root: &Path, providers: &[ProviderSpec]) -> Result<()> {
    let missing_files = missing_required_files(root);
    if !missing_files.is_empty() {
        bail!(
            "missing required harness source files under {}: {}",
            root.display(),
            missing_files.join(", ")
        )
    }
    let layout = SourceLayout::new(root);
    let missing_directories: Vec<_> = layout
        .required_directories_for(providers)
        .into_iter()
        .filter(|path| !path.is_dir())
        .collect();
    if !missing_directories.is_empty() {
        bail!(
            "missing required harness source directories: {}",
            missing_directories
                .iter()
                .map(|directory| directory.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
    read_manifest(root)?;
    Ok(())
}

fn missing_required_files(root: &Path) -> Vec<&'static str> {
    SourceLayout::new(root)
        .required_files()
        .filter(|(_, path)| !path.is_file())
        .map(|(relative, _)| relative)
        .collect()
}

/// The configs `providers` merge into, as they are installed under `prefix`.
///
/// A config that cannot be parsed is an error here, at the boundary. The sidecar is left unread:
/// installation has always merged into the config alone, and the sidecar it would write sits in the
/// staging directory, which is never published.
///
/// # Errors
///
/// Returns an error naming an installed config that exists and cannot be parsed.
pub(crate) fn load_installed(
    prefix: &Path,
    providers: &[ProviderSpec],
) -> Result<InstalledConfigs> {
    let mut installed = InstalledConfigs::default();
    for (config, relative) in merged_configs(providers) {
        installed.insert(
            relative,
            read_unsynced_config(&config, &prefix.join(relative))?,
        );
    }
    Ok(installed)
}

/// The configs rendering merges into the installed ones, with the path the registry names them by.
fn merged_configs(providers: &[ProviderSpec]) -> impl Iterator<Item = (ConfigSpec, &'static str)> {
    providers
        .iter()
        .filter_map(|spec| spec.config.zip(spec.config_path()))
        .filter(|(config, _)| merges_installed(config))
}

fn read_unsynced_config(config: &ConfigSpec, path: &Path) -> Result<Installed> {
    let text = read_text_if_present(path)?;
    let text = InstalledText {
        config: text.as_deref(),
        sidecar: None,
    };
    Installed::parse(&config.sync, path, text)
}

fn merges_installed(config: &ConfigSpec) -> bool {
    config.rendered.merges_into_installed() || config.sync.replaces_every_key()
}

/// The rendered config at `path`, parsed for `config`.
///
/// # Errors
///
/// Returns an error naming `path` when it cannot be read or parsed.
pub(crate) fn read_generated_config(config: &ConfigSpec, path: &Path) -> Result<ConfigDocument> {
    let text = read_text(path)?;
    parse_generated(&config.sync, path, &text)
}

/// The installed config at `path` and its sidecar, parsed for `config`.
///
/// # Errors
///
/// Returns an error naming `path` when it exists and cannot be parsed.
pub(crate) fn read_installed_config(config: &ConfigSpec, path: &Path) -> Result<Installed> {
    let config_text = read_text_if_present(path)?;
    let sidecar_text = config
        .sync
        .sidecar_file()
        .map(|file| crate::config_sync::installed::sidecar_path(path, file))
        .and_then(|path| read_text_if_present(&path).ok().flatten());
    Installed::parse(
        &config.sync,
        path,
        InstalledText {
            config: config_text.as_deref(),
            sidecar: sidecar_text.as_deref(),
        },
    )
}

/// The real path of `path`, which must exist.
pub(crate) fn real_path(path: &Path) -> Result<PathBuf> {
    std::fs::canonicalize(path).with_context(|| format!("failed to resolve {}", path.display()))
}

/// Checks that `files` are files and `directories` are directories, reporting the first that is
/// not.
///
/// # Errors
///
/// Returns an error naming the first missing or invalid path.
pub(crate) fn verify_paths(files: &[PathBuf], directories: &[PathBuf]) -> Result<()> {
    for path in files {
        ensure!(
            path.is_file(),
            "missing or invalid harness file: {}",
            path.display()
        );
    }
    for path in directories {
        ensure!(
            path.is_dir(),
            "missing or invalid harness directory: {}",
            path.display()
        );
    }
    Ok(())
}

fn load_command_permissions(layout: &SourceLayout<'_>) -> Result<CommandPermissions> {
    let path = layout.command_permissions();
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read command permissions {}", path.display()))?;
    CommandPermissions::parse(&path, &text)
}

fn load_regex_policy(path: &Path) -> Result<RegexPolicy> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read command regex policy {}", path.display()))?;
    RegexPolicy::parse(path, &text)
}

/// The allowed-commands policy is read by the hooks at run time; installation only copies it, but a
/// malformed one is refused here.
fn validate_allowed_commands(layout: &SourceLayout<'_>) -> Result<()> {
    load_regex_policy(&layout.allowed_command_rules()).map(drop)
}

fn load_secret_paths(layout: &SourceLayout<'_>) -> Result<SecretPathPolicy> {
    let path = layout.secret_path_policy();
    SecretPathPolicy::parse(&path, &read_text(&path)?)
}

fn load_provider_configs(
    root: &Path,
    providers: &[ProviderSpec],
) -> Result<BTreeMap<String, ConfigDocument>> {
    providers
        .iter()
        .filter_map(|spec| spec.config)
        .filter_map(|config| source_config_path(&config).map(|relative| (config, relative)))
        .map(|(config, relative)| {
            let document = load_source_config(&config, &root.join(relative))?;
            Ok((relative.to_owned(), document))
        })
        .collect()
}

fn source_config_path(config: &ConfigSpec) -> Option<&'static str> {
    match config.rendered {
        RenderedConfig::SourceFile(relative) => Some(relative),
        RenderedConfig::WithHooks
        | RenderedConfig::Fixed(_)
        | RenderedConfig::SourceTomlWithProtection => None,
    }
}

/// The source file of a provider config, or a blank document when the source has none.
fn load_source_config(config: &ConfigSpec, path: &Path) -> Result<ConfigDocument> {
    match read_text_if_present(path)? {
        Some(text) => parse_generated(&config.sync, path, &text),
        None => Ok(ConfigDocument::blank(config.sync.format)),
    }
}

fn load_directories(
    layout: &SourceLayout<'_>,
    providers: &[ProviderSpec],
) -> Result<BTreeMap<PathBuf, SourceDir>> {
    layout
        .synced_directories_for(providers)
        .into_iter()
        .map(|path| Ok((path.clone(), source_dir(path)?)))
        .collect()
}

fn require_directories(
    layout: &SourceLayout<'_>,
    providers: &[ProviderSpec],
    directories: &BTreeMap<PathBuf, SourceDir>,
) -> Result<()> {
    let missing = layout
        .required_directories_for(providers)
        .into_iter()
        .find(|path| !directories.get(path).is_some_and(|dir| dir.present));
    match missing {
        Some(path) => bail!(
            "missing required harness source directory: {}",
            path.display()
        ),
        None => Ok(()),
    }
}

fn source_dir(path: PathBuf) -> Result<SourceDir> {
    if !path.is_dir() {
        return Ok(SourceDir::absent(path));
    }
    let files = regular_files(&path)?;
    Ok(SourceDir {
        path,
        present: true,
        files,
    })
}

fn load_skills(layout: &SourceLayout<'_>) -> Result<SourceSkills> {
    let rendering_path = layout.skill_rendering();
    let rendering = read_text_if_present(&rendering_path)?;
    let files = children(&layout.skills())?
        .into_iter()
        .filter(Child::is_real_directory)
        .filter(|child| child.path.join("SKILL.md").is_file())
        .map(|child| skill_files(child.path))
        .collect::<Result<Vec<_>>>()?;
    SourceSkills::parse(
        rendering
            .as_deref()
            .map(|text| (rendering_path.as_path(), text)),
        files,
    )
}

fn skill_files(directory: PathBuf) -> Result<SkillFiles> {
    let skill_text = read_text(&directory.join("SKILL.md"))?;
    let support = children(&directory)?
        .into_iter()
        .filter(|child| child.name != "SKILL.md")
        .filter_map(support_entry)
        .collect();
    Ok(SkillFiles {
        directory,
        skill_text,
        support,
    })
}

/// A skill's support file or directory; symlinks are skipped, so a skill never reaches outside its
/// own directory.
fn support_entry(child: Child) -> Option<SupportEntry> {
    if child.is_real_directory() {
        return Some(SupportEntry::Directory {
            name: child.name,
            path: child.path,
        });
    }
    child.is_regular_file().then_some(SupportEntry::File {
        name: child.name,
        path: child.path,
    })
}

fn external_skill_files(skills: &[ExternalSkill]) -> Vec<bool> {
    skills
        .iter()
        .map(|skill| is_regular_file(&skill.skill_file()))
        .collect()
}

mod bundle;
mod files;

use bundle::load_bundles;
use files::{
    Child, children, is_regular_file, read_json, read_text, read_text_if_present, read_toml,
};
pub(crate) use files::{regular_files, shell_scripts};

#[cfg(test)]
mod tests;
