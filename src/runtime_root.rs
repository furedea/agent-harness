use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

use crate::provider::spec::PROVIDERS;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RuntimeRoot {
    Home,
    Directory(PathBuf),
}

impl RuntimeRoot {
    pub(crate) fn home() -> Self {
        Self::Home
    }

    pub(crate) fn directory(path: PathBuf) -> Result<Self> {
        Ok(Self::Directory(absolute_path(path)?))
    }

    pub(crate) fn path(&self, relative: &Path) -> String {
        let path = match self {
            Self::Home => Path::new("~").join(relative),
            Self::Directory(root) => root.join(relative),
        };
        let path = path.to_string_lossy().into_owned();
        #[cfg(windows)]
        let path = path.replace('\\', "/");
        path
    }

    /// The project directory of a project-local install, which Codex treats as the workspace.
    pub(crate) fn project_directory(&self) -> Option<String> {
        match self {
            Self::Home => None,
            Self::Directory(_) => Some(self.path(Path::new(""))),
        }
    }

    pub(crate) fn relocate_command(&self, command: &str) -> String {
        match self {
            Self::Home => command.to_owned(),
            Self::Directory(root) => relocate_command_under(root, command),
        }
    }
}

fn absolute_path(path: PathBuf) -> Result<PathBuf> {
    if !path.is_absolute() {
        bail!("runtime root must be an absolute path: {}", path.display());
    }
    Ok(path)
}

fn relocate_command_under(root: &Path, command: &str) -> String {
    let homes = provider_home_references();
    if !homes.iter().any(|home| command.contains(home)) {
        return command.to_owned();
    }

    let root = root.to_string_lossy();
    let relocated = replace_home_references(command, &root);
    format!(
        "export AGENT_HARNESS_ROOT={}; {relocated}",
        shell_quote(&root)
    )
}

/// `$HOME/<home_dir>/` for every provider, so a new provider is relocated without an edit here.
fn provider_home_references() -> Vec<String> {
    PROVIDERS
        .iter()
        .map(|spec| format!("$HOME/{}/", spec.home_dir))
        .collect()
}

fn replace_home_references(command: &str, root: &str) -> String {
    let quoted = [
        ("\"", double_quote_escape(root)),
        ("'", single_quote_escape(root)),
    ];
    let command = quoted
        .iter()
        .fold(command.to_owned(), |command, (quote, root)| {
            replace_after(&command, quote, root)
        });
    replace_after(&command, "", &shell_quote(root))
}

/// Replaces `$HOME` in each provider home that follows `prefix` with `root`.
fn replace_after(command: &str, prefix: &str, root: &str) -> String {
    provider_home_references()
        .iter()
        .fold(command.to_owned(), |command, home| {
            let rest = &home["$HOME".len()..];
            command.replace(&format!("{prefix}{home}"), &format!("{prefix}{root}{rest}"))
        })
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn single_quote_escape(value: &str) -> String {
    value.replace('\'', "'\"'\"'")
}

fn double_quote_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('$', "\\$")
        .replace('`', "\\`")
        .replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn directory_exposes_the_runtime_root_to_compound_hook_commands() -> Result<()> {
        let root = RuntimeRoot::directory(PathBuf::from("/work/project with spaces"))?;
        let command = root.relocate_command(
            "if [ -n \"$HOME/.claude/hooks/guard.sh\" ]; then printenv AGENT_HARNESS_ROOT; fi",
        );

        let output = std::process::Command::new("/bin/sh")
            .args(["-c", &command])
            .output()?;

        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            String::from_utf8(output.stdout)?,
            "/work/project with spaces\n"
        );
        Ok(())
    }

    #[test]
    fn directory_relocates_quoted_and_unquoted_hook_paths() -> Result<()> {
        let root = RuntimeRoot::directory(PathBuf::from("/work/project with spaces"))?;

        assert_eq!(
            root.relocate_command(
                "bash \"$HOME/.codex/hooks/adapt.sh\" $HOME/.claude/hooks/guard.sh"
            ),
            "export AGENT_HARNESS_ROOT='/work/project with spaces'; bash \"/work/project with spaces/.codex/hooks/adapt.sh\" '/work/project with spaces'/.claude/hooks/guard.sh",
        );
        Ok(())
    }

    fn hook_command(home_dir: &str) -> String {
        format!("bash \"$HOME/{home_dir}/hooks/run.sh\"")
    }

    fn relocated_hook_command(home_dir: &str) -> String {
        format!(
            "export AGENT_HARNESS_ROOT='/work/project'; bash \"/work/project/{home_dir}/hooks/run.sh\""
        )
    }

    #[test]
    fn directory_relocates_the_hook_commands_of_every_provider() -> Result<()> {
        let root = RuntimeRoot::directory(PathBuf::from("/work/project"))?;

        let relocated: Vec<_> = PROVIDERS
            .iter()
            .map(|spec| root.relocate_command(&hook_command(spec.home_dir)))
            .collect();
        let expected: Vec<_> = PROVIDERS
            .iter()
            .map(|spec| relocated_hook_command(spec.home_dir))
            .collect();

        assert_eq!(relocated, expected);
        Ok(())
    }

    #[test]
    fn directory_leaves_commands_outside_every_provider_home_alone() -> Result<()> {
        let root = RuntimeRoot::directory(PathBuf::from("/work/project"))?;

        assert_eq!(
            root.relocate_command("bash \"$HOME/.unknown/hooks/run.sh\""),
            "bash \"$HOME/.unknown/hooks/run.sh\""
        );
        Ok(())
    }

    #[test]
    fn directory_keeps_single_quoted_hook_paths_inside_their_quotes() -> Result<()> {
        let root = RuntimeRoot::directory(PathBuf::from("/work/project with spaces"))?;

        assert_eq!(
            root.relocate_command("bash -c '$HOME/.pi/hooks/run.sh'"),
            "export AGENT_HARNESS_ROOT='/work/project with spaces'; bash -c '/work/project with spaces/.pi/hooks/run.sh'",
        );
        Ok(())
    }
}
