use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

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
        if !path.is_absolute() {
            bail!("runtime root must be an absolute path: {}", path.display());
        }
        Ok(Self::Directory(path))
    }

    pub(crate) fn path(&self, relative: &Path) -> String {
        let relative = relative.to_string_lossy().replace('\\', "/");
        match self {
            Self::Home => format!("~/{relative}"),
            Self::Directory(root) => root.join(relative).to_string_lossy().replace('\\', "/"),
        }
    }

    pub(crate) fn relocate_command(&self, command: &str) -> String {
        let Self::Directory(root) = self else {
            return command.to_owned();
        };
        if !command.contains("$HOME/.claude/") && !command.contains("$HOME/.codex/") {
            return command.to_owned();
        }

        let root = root.to_string_lossy();
        let relocated = replace_home_references(command, &root);
        format!("AGENT_HARNESS_ROOT={} {relocated}", shell_quote(&root))
    }
}

fn replace_home_references(command: &str, root: &str) -> String {
    let double_quoted_root = double_quote_escape(root);
    let command = command
        .replace(
            "\"$HOME/.claude/",
            &format!("\"{double_quoted_root}/.claude/"),
        )
        .replace(
            "\"$HOME/.codex/",
            &format!("\"{double_quoted_root}/.codex/"),
        );
    let shell_root = shell_quote(root);
    command
        .replace("$HOME/.claude/", &format!("{shell_root}/.claude/"))
        .replace("$HOME/.codex/", &format!("{shell_root}/.codex/"))
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
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

    #[test]
    fn directory_relocates_quoted_and_unquoted_hook_paths() -> Result<()> {
        let root = RuntimeRoot::directory(PathBuf::from("/work/project with spaces"))?;

        assert_eq!(
            root.relocate_command(
                "bash \"$HOME/.codex/hooks/adapt.sh\" $HOME/.claude/hooks/guard.sh"
            ),
            "AGENT_HARNESS_ROOT='/work/project with spaces' bash \"/work/project with spaces/.codex/hooks/adapt.sh\" '/work/project with spaces'/.claude/hooks/guard.sh",
        );
        Ok(())
    }
}
