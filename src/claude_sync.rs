use std::path::Path;

use anyhow::{Result, bail};

use crate::{fs_ops, generation::claude_config};

pub(crate) fn sync(source: &Path, skills_source: &Path, target: &Path) -> Result<()> {
    let instructions = source.join("CLAUDE.md");
    let settings = source.join("settings.json");
    let hooks = source.join("hooks");
    required_file(&instructions)?;
    required_file(&settings)?;
    required_directory(&hooks)?;
    required_directory(skills_source)?;

    fs_ops::materialize_file(&instructions, &target.join("CLAUDE.md"))?;
    fs_ops::materialize_dir(&hooks, &target.join("hooks"))?;
    fs_ops::materialize_dir(skills_source, &target.join("skills"))?;
    claude_config::sync_settings(&settings, &target.join("settings.json"))
}

fn required_file(path: &Path) -> Result<()> {
    if !path.is_file() {
        bail!("missing Claude file source: {}", path.display());
    }
    Ok(())
}

fn required_directory(path: &Path) -> Result<()> {
    if !path.is_dir() {
        bail!("missing Claude directory source: {}", path.display());
    }
    Ok(())
}
