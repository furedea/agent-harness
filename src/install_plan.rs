use crate::{file_op::FileOp, layout::InstalledLayout};

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
        .map(|(source, target)| FileOp::CopyTree { source, target });
    let files = rendered
        .managed_files_for(providers)
        .into_iter()
        .zip(installed.managed_files_for(providers))
        .map(|(source, target)| FileOp::CopyFile { source, target });
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
        .map(|asset| FileOp::write(out.join(asset.destination), asset.content))
        .collect()
}
