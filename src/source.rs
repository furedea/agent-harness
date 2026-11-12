#![expect(
    clippy::disallowed_methods,
    reason = "locating the source starts from the environment, the working directory, and the executable"
)]

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::{
    file_op::{FileMode, FileOp},
    profile::Profile,
    provider::spec::PROVIDERS,
    shell::{self, TemporaryDirectory, has_required_files, validate_source_layout},
};

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
    /// Keeps a materialized packaged source alive; it is removed when the root is dropped.
    _materialized: Option<TemporaryDirectory>,
}

impl SourceRoot {
    fn external(path: PathBuf) -> Self {
        Self {
            path,
            _materialized: None,
        }
    }

    pub(crate) fn as_path(&self) -> &Path {
        &self.path
    }
}

pub(crate) fn resolve_source(
    explicit_source: Option<PathBuf>,
    profile: Profile,
) -> Result<SourceRoot> {
    if let Some(source) = explicit_source {
        return required_source(&source, "--source", profile);
    }

    if let Some(source) = std::env::var_os(ENV_SOURCE).map(PathBuf::from) {
        return required_source(&source, ENV_SOURCE, profile);
    }

    let installed_source = installed_source_dirs()?
        .iter()
        .find_map(|source| profile_source(source, profile));
    if let Some(profile_source) = installed_source {
        validate_source_tree(&profile_source)?;
        return Ok(SourceRoot::external(profile_source));
    }

    let cwd = std::env::current_dir().context("failed to read current directory")?;
    if let Some(profile_source) = profile_source(&cwd, profile) {
        validate_source_tree(&profile_source)?;
        return Ok(SourceRoot::external(profile_source));
    }

    materialize_packaged_source(profile)
}

fn required_source(path: &Path, label: &str, profile: Profile) -> Result<SourceRoot> {
    let profile_source = profile_source(path, profile).ok_or_else(|| {
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
    let Some(binary_dir) = executable.parent() else {
        return Vec::new();
    };
    let mut candidates = vec![binary_dir.join("share/agent-harness")];
    if let Some(prefix) = binary_dir.parent() {
        candidates.push(prefix.join("share/agent-harness"));
    }
    candidates.sort();
    candidates.dedup();
    candidates
}

fn materialize_packaged_source(profile: Profile) -> Result<SourceRoot> {
    let root = TemporaryDirectory::create()?;
    shell::execute(&packaged_file_ops(root.path()))?;

    let profile_source = profile_source(root.path(), profile).ok_or_else(|| {
        anyhow::anyhow!(
            "packaged assets do not contain the {} profile",
            profile.directory_name(),
        )
    })?;
    validate_source_tree(&profile_source)?;
    Ok(SourceRoot {
        path: profile_source,
        _materialized: Some(root),
    })
}

fn packaged_file_ops(root: &Path) -> Vec<FileOp> {
    PACKAGED_FILES
        .iter()
        .map(|file| FileOp::WriteFile {
            target: root.join(file.path),
            content: file.content.to_vec(),
            mode: FileMode::Bits(file.mode),
        })
        .collect()
}

fn profile_source(root: &Path, profile: Profile) -> Option<PathBuf> {
    let profile_root = root.join("profiles").join(profile.directory_name());
    if has_required_files(&profile_root) {
        return Some(profile_root);
    }

    has_required_files(root).then(|| root.to_path_buf())
}

fn validate_source_tree(path: &Path) -> Result<()> {
    validate_source_layout(path, PROVIDERS)
}

#[cfg(test)]
mod tests;
