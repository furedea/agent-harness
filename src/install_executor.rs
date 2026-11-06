use anyhow::Result;

use crate::{filesystem::FileSystem, install_plan::FileOp};

pub(crate) fn execute(filesystem: &impl FileSystem, plan: &[FileOp]) -> Result<()> {
    for operation in plan {
        apply(filesystem, operation)?;
    }
    Ok(())
}

fn apply(filesystem: &impl FileSystem, operation: &FileOp) -> Result<()> {
    match operation {
        FileOp::WriteFile { target, content } => filesystem.write_file(target, content),
        FileOp::ReplaceDirectory { source, target } => filesystem.replace_directory(source, target),
        FileOp::InstallFile { source, target } => {
            let content = filesystem.read_file(source)?;
            filesystem.write_file(target, &content)
        }
    }
}
