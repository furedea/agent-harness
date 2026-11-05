use std::path::PathBuf;

use crate::layout::InstalledLayout;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum FileOp {
    WriteFile { target: PathBuf, content: Vec<u8> },
    ReplaceDirectory { source: PathBuf, target: PathBuf },
    InstallFile { source: PathBuf, target: PathBuf },
}

/// Plan publication of a complete rendered installation without accessing the filesystem.
pub(crate) fn plan_install(
    rendered: &InstalledLayout<'_>,
    installed: &InstalledLayout<'_>,
) -> Vec<FileOp> {
    let directories = rendered
        .managed_directories()
        .into_iter()
        .zip(installed.managed_directories())
        .map(|(source, target)| FileOp::ReplaceDirectory { source, target });
    let files = rendered
        .managed_files()
        .into_iter()
        .zip(installed.managed_files())
        .map(|(source, target)| FileOp::InstallFile { source, target });
    directories.chain(files).collect()
}

#[cfg(test)]
mod tests;

pub(crate) fn plan_assets(out: &std::path::Path) -> Vec<FileOp> {
    crate::provider::spec::PROVIDERS
        .iter()
        .flat_map(|spec| spec.assets.iter())
        .map(|asset| FileOp::WriteFile {
            target: out.join(asset.destination),
            content: asset.content.as_bytes().to_vec(),
        })
        .collect()
}
