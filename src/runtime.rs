#![expect(
    clippy::disallowed_methods,
    reason = "verification asks the environment which commands the PATH can run"
)]

use std::ffi::OsStr;
use std::path::Path;

use anyhow::{Result, bail};

use crate::source_model::Source;

pub(crate) fn verify(source: &Source) -> Result<()> {
    let path = std::env::var_os("PATH");
    verify_source_with_path(source, path.as_deref())
}

fn verify_source_with_path(source: &Source, path: Option<&OsStr>) -> Result<()> {
    let missing = source
        .manifest
        .runtime_commands()
        .iter()
        .filter(|command| !is_command_available(command, path))
        .map(String::as_str)
        .collect::<Vec<_>>();

    if !missing.is_empty() {
        bail!(
            "missing required runtime commands for source {}: {}",
            source.root().display(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_uses_runtime_commands_declared_by_the_source() {
        let root = std::env::temp_dir().join(format!(
            "agent-harness-runtime-manifest-{}",
            std::process::id(),
        ));
        let path = root.join("bin");
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(
            root.join("manifest.json"),
            r#"{"version":1,"runtime_commands":["missing-agent-harness-command"]}"#,
        )
        .unwrap();
        let source = crate::shell::fixtures::load_source(&root).unwrap();

        let error = verify_source_with_path(&source, Some(path.as_os_str())).unwrap_err();

        assert!(error.to_string().contains("missing-agent-harness-command"));
        std::fs::remove_dir_all(root).unwrap();
    }
}
