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
    plan_install_for(rendered, installed, crate::provider::spec::PROVIDERS)
}

pub(crate) fn plan_install_for(
    rendered: &InstalledLayout<'_>,
    installed: &InstalledLayout<'_>,
    providers: &[crate::provider::spec::ProviderSpec],
) -> Vec<FileOp> {
    let directories = rendered
        .managed_directories_for(providers)
        .into_iter()
        .zip(installed.managed_directories_for(providers))
        .map(|(source, target)| FileOp::ReplaceDirectory { source, target });
    let files = rendered
        .managed_files_for(providers)
        .into_iter()
        .zip(installed.managed_files_for(providers))
        .map(|(source, target)| FileOp::InstallFile { source, target });
    directories.chain(files).collect()
}

#[cfg(test)]
mod tests;

pub(crate) fn plan_assets_for(
    out: &std::path::Path,
    providers: &[crate::provider::spec::ProviderSpec],
) -> Vec<FileOp> {
    providers
        .iter()
        .flat_map(|spec| spec.hook.bridge.assets.iter())
        .map(|asset| FileOp::WriteFile {
            target: out.join(asset.destination),
            content: asset.content.as_bytes().to_vec(),
        })
        .collect()
}
