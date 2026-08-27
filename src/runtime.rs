use std::ffi::OsStr;
use std::path::Path;

use anyhow::{Result, bail};

use crate::profile::Profile;

pub(crate) fn verify(profile: Profile) -> Result<()> {
    let path = std::env::var_os("PATH");
    let missing = profile
        .required_runtime_commands()
        .iter()
        .filter(|command| !is_command_available(command, path.as_deref()))
        .copied()
        .collect::<Vec<_>>();

    if !missing.is_empty() {
        bail!(
            "missing required runtime commands for {} profile: {}",
            profile.directory_name(),
            missing.join(", "),
        );
    }

    Ok(())
}

fn is_command_available(command: &str, path: Option<&OsStr>) -> bool {
    path.into_iter()
        .flat_map(std::env::split_paths)
        .map(|directory| directory.join(command))
        .any(|candidate| is_executable(&candidate))
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    path.metadata()
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}
