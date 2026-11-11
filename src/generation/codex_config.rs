use std::path::Path;

use anyhow::{Context, Result};
use toml_edit::{DocumentMut, Item, TableLike};

use crate::{
    file_op::FileOp,
    generation::{
        external_hooks::{ExternalHookBundle, OpenBundle},
        protection,
    },
    layout::SourceLayout,
    runtime_root::RuntimeRoot,
    snapshot::{Snapshot, Tree},
};

pub(crate) fn config_source_op(
    source: Tree<'_>,
    out: &Path,
    external_hooks: &[ExternalHookBundle],
) -> Result<FileOp> {
    Ok(FileOp::write(
        out,
        config_source_content(source, external_hooks)?,
    ))
}

pub(crate) fn generated_config_source(
    context: crate::generation::GenerationContext<'_>,
) -> Result<DocumentMut> {
    config_source_content_for_runtime(context)?
        .parse::<DocumentMut>()
        .context("failed to parse generated Codex config source")
}

fn config_source_content(
    source: Tree<'_>,
    external_hooks: &[ExternalHookBundle],
) -> Result<String> {
    config_source_content_for_runtime(crate::generation::GenerationContext {
        source,
        external_hooks,
        runtime_root: &RuntimeRoot::home(),
        dsh_profile: "web",
    })
}

fn config_source_content_for_runtime(
    context: crate::generation::GenerationContext<'_>,
) -> Result<String> {
    let crate::generation::GenerationContext {
        source,
        external_hooks,
        ..
    } = context;
    let base_path = SourceLayout::new(source.root()).codex_config();
    let base = source
        .files()
        .read_text(&base_path)
        .with_context(|| format!("failed to read TOML file {}", base_path.display()))?;
    let mut document = base
        .parse::<DocumentMut>()
        .with_context(|| format!("failed to parse TOML file {}", base_path.display()))?;
    // Merge rather than concatenate: the source may already declare guarded entries, such as
    // the agent GitHub token's read denials.
    let protection = protection::codex_config_fragment_for_runtime(context)?
        .parse::<DocumentMut>()
        .context("failed to parse generated Codex config")?;
    for (key, item) in protection.iter() {
        merge_item(document.entry(key).or_insert(Item::None), item);
    }
    for bundle in external_hooks {
        merge_external_config(&mut document, &bundle.open(source.files())?, source.files())?;
    }
    Ok(document.to_string())
}

fn merge_external_config(
    document: &mut DocumentMut,
    bundle: &OpenBundle<'_>,
    files: &Snapshot,
) -> Result<()> {
    let Some(path) = bundle.codex_config_path() else {
        return Ok(());
    };
    let generated = read_toml_document(files, &path)?;
    merge_external_features(document, &generated, &path)
}

fn merge_external_features(
    target: &mut DocumentMut,
    source: &DocumentMut,
    path: &Path,
) -> Result<()> {
    for (key, item) in source.iter() {
        anyhow::ensure!(
            key == "features",
            "external hook config {} contains unsupported top-level key: {key}",
            path.display(),
        );
        merge_item(&mut target[key], item);
    }
    Ok(())
}

fn merge_item(target: &mut Item, source: &Item) {
    match (target.as_table_like_mut(), source.as_table_like()) {
        (Some(target), Some(source)) => merge_tables(target, source),
        _ => *target = source.clone(),
    }
}

fn merge_tables(target: &mut dyn TableLike, source: &dyn TableLike) {
    for (key, item) in source.iter() {
        merge_item(target.entry(key).or_insert(Item::None), item);
    }
}

fn read_toml_document(files: &Snapshot, path: &Path) -> Result<DocumentMut> {
    let content = files
        .read_text(path)
        .with_context(|| format!("failed to read TOML file {}", path.display()))?;
    content
        .parse::<DocumentMut>()
        .with_context(|| format!("failed to parse TOML file {}", path.display()))
}

#[cfg(test)]
mod tests;
