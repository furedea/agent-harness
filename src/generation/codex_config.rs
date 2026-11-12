use std::path::Path;

use anyhow::{Context, Result};
use toml_edit::{DocumentMut, Item, TableLike};

use crate::{
    file_op::FileOp,
    generation::{GenerationContext, protection},
    source_model::{HookBundle, Source},
};

pub(crate) fn config_source_op(source: &Source, out: &Path) -> Result<FileOp> {
    Ok(FileOp::write(out, config_source_content(source)?))
}

pub(crate) fn generated_config_source(context: GenerationContext<'_>) -> Result<DocumentMut> {
    config_source_content_for_runtime(context)?
        .parse::<DocumentMut>()
        .context("failed to parse generated Codex config source")
}

fn config_source_content(source: &Source) -> Result<String> {
    config_source_content_for_runtime(GenerationContext::home(source))
}

fn config_source_content_for_runtime(context: GenerationContext<'_>) -> Result<String> {
    let mut document = context.source.codex_config.clone();
    // Merge rather than concatenate: the source may already declare guarded entries, such as
    // the agent GitHub token's read denials.
    let mut protection = protection::codex_config_fragment_for_runtime(context)?
        .parse::<DocumentMut>()
        .context("failed to parse generated Codex config")?;
    reconcile::guarded_filesystem(&mut document, &mut protection);
    for (key, item) in protection.iter() {
        merge_item(document.entry(key).or_insert(Item::None), item);
    }
    for bundle in &context.source.hook_bundles {
        merge_external_config(&mut document, bundle)?;
    }
    Ok(document.to_string())
}

fn merge_external_config(document: &mut DocumentMut, bundle: &HookBundle) -> Result<()> {
    let Some((path, generated)) = &bundle.codex_config else {
        return Ok(());
    };
    merge_external_features(document, generated, path)
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

mod reconcile;

#[cfg(test)]
mod tests;
