use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};

use crate::{layout::SourceLayout, profile::Profile};

const ENV_SOURCE: &str = "AGENT_HARNESS_SOURCE";

#[derive(Debug)]
struct PackagedFile {
    path: &'static str,
    mode: u32,
    content: &'static [u8],
}

include!(concat!(env!("OUT_DIR"), "/packaged_assets.rs"));

#[derive(Debug)]
pub(crate) struct SourceRoot {
    path: PathBuf,
    cleanup: Option<PathBuf>,
}

impl SourceRoot {
    fn external(path: PathBuf) -> Self {
        Self {
            path,
            cleanup: None,
        }
    }

    pub(crate) fn as_path(&self) -> &Path {
        &self.path
    }
}

impl Drop for SourceRoot {
    fn drop(&mut self) {
        if let Some(path) = &self.cleanup {
            let _ = std::fs::remove_dir_all(path);
        }
    }
}

pub(crate) fn resolve_source(
    explicit_source: Option<PathBuf>,
    profile: Profile,
) -> Result<SourceRoot> {
    if let Some(source) = explicit_source {
        return required_source(source, "--source", profile);
    }

    if let Some(source) = std::env::var_os(ENV_SOURCE).map(PathBuf::from) {
        return required_source(source, ENV_SOURCE, profile);
    }

    for source in installed_source_dirs()? {
        if let Some(profile_source) = profile_source(&source, profile) {
            return Ok(SourceRoot::external(profile_source));
        }
    }

    let cwd = std::env::current_dir().context("failed to read current directory")?;
    if let Some(profile_source) = profile_source(&cwd, profile) {
        return Ok(SourceRoot::external(profile_source));
    }

    materialize_packaged_source(profile)
}

fn required_source(path: PathBuf, label: &str, profile: Profile) -> Result<SourceRoot> {
    let profile_source = profile_source(&path, profile).ok_or_else(|| {
        anyhow::anyhow!(
            "{label} does not contain the {} agent-harness profile: {}",
            profile.directory_name(),
            path.display(),
        )
    })?;
    validate_source_tree(&profile_source).with_context(|| {
        format!(
            "{label} does not point to an agent-harness source tree: {}",
            path.display()
        )
    })?;
    Ok(SourceRoot::external(profile_source))
}

fn installed_source_dirs() -> Result<Vec<PathBuf>> {
    let executable = std::env::current_exe().context("failed to locate current executable")?;
    Ok(installed_source_dirs_for_executable(&executable))
}

fn installed_source_dirs_for_executable(executable: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(binary_dir) = executable.parent() {
        candidates.push(binary_dir.join("share/agent-harness"));
        if let Some(prefix) = binary_dir.parent() {
            candidates.push(prefix.join("share/agent-harness"));
        }
    }
    candidates.sort();
    candidates.dedup();
    candidates
}

fn materialize_packaged_source(profile: Profile) -> Result<SourceRoot> {
    let root = std::env::temp_dir().join(format!(
        "agent-harness-source-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    std::fs::create_dir_all(&root)
        .with_context(|| format!("failed to create {}", root.display()))?;

    for file in PACKAGED_FILES {
        write_packaged_file(&root, file)?;
    }

    let profile_source = profile_source(&root, profile).ok_or_else(|| {
        anyhow::anyhow!(
            "packaged assets do not contain the {} profile",
            profile.directory_name(),
        )
    })?;
    validate_source_tree(&profile_source)?;
    Ok(SourceRoot {
        path: profile_source,
        cleanup: Some(root),
    })
}

fn write_packaged_file(root: &Path, file: &PackagedFile) -> Result<()> {
    let path = root.join(file.path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    std::fs::write(&path, file.content)
        .with_context(|| format!("failed to write {}", path.display()))?;
    set_file_mode(&path, file.mode)
}

#[cfg(unix)]
fn set_file_mode(path: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let permissions = std::fs::Permissions::from_mode(mode);
    std::fs::set_permissions(path, permissions)
        .with_context(|| format!("failed to set permissions on {}", path.display()))
}

#[cfg(not(unix))]
fn set_file_mode(_path: &Path, _mode: u32) -> Result<()> {
    Ok(())
}

fn unique_suffix() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos())
}

fn profile_source(root: &Path, profile: Profile) -> Option<PathBuf> {
    let profile_root = root.join("profiles").join(profile.directory_name());
    if SourceLayout::new(&profile_root).is_complete() {
        return Some(profile_root);
    }

    SourceLayout::new(root)
        .is_complete()
        .then(|| root.to_path_buf())
}

fn validate_source_tree(path: &Path) -> Result<()> {
    let missing = SourceLayout::new(path).missing_required_files();

    if missing.is_empty() {
        return Ok(());
    }

    bail!(
        "missing required harness source files under {}: {}",
        path.display(),
        missing.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn installed_source_dirs_include_release_tarball_layout_next_to_binary() {
        let candidates = installed_source_dirs_for_executable(Path::new(
            "/home/user/.local/agent-harness/agent-harness",
        ));

        assert_eq!(
            candidates,
            vec![
                PathBuf::from("/home/user/.local/agent-harness/share/agent-harness"),
                PathBuf::from("/home/user/.local/share/agent-harness"),
            ],
        );
    }

    #[test]
    fn installed_source_dirs_include_nix_style_prefix_layout() {
        let candidates = installed_source_dirs_for_executable(Path::new(
            "/nix/store/hash-agent-harness/bin/agent-harness",
        ));

        assert!(candidates.contains(&PathBuf::from(
            "/nix/store/hash-agent-harness/share/agent-harness"
        )));
    }
}
