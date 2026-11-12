use std::path::Path;

use anyhow::{Context, Result};
use toml_edit::{DocumentMut, Item, TableLike};

use crate::{
    file_op::FileOp,
    generation::{
        GenerationContext,
        guarded_conflict::{self, SourceConfig},
        protection,
    },
    layout::SourceLayout,
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
    let protection = protection::codex_config_fragment_for_runtime(context)?
        .parse::<DocumentMut>()
        .context("failed to parse generated Codex config")?;
    check_guarded_conflicts(context, &protection)?;
    keep_scalar_workspace_grants(&mut document, &protection);
    for (key, item) in protection.iter() {
        merge_item(document.entry(key).or_insert(Item::None), item);
    }
    for bundle in &context.source.hook_bundles {
        merge_external_config(&mut document, bundle)?;
    }
    Ok(document.to_string())
}

fn check_guarded_conflicts(context: GenerationContext<'_>, generated: &DocumentMut) -> Result<()> {
    let source = context.source;
    let path = SourceLayout::new(source.root()).codex_config();
    guarded_conflict::check(
        generated,
        Some(SourceConfig {
            path: &path,
            document: &source.codex_config,
        }),
        context.runtime_root.project_directory().as_deref(),
    )
}

/// A scalar source grant on the workspace becomes a `"."` entry when the generated fragment
/// scopes entries under the same key, because the merge would replace the scalar with the table.
fn keep_scalar_workspace_grants(source: &mut DocumentMut, generated: &DocumentMut) {
    for key in ["workspace_roots", "project_roots"] {
        scope_scalar_grant(source, generated, &format!(":{key}"));
    }
}

fn scope_scalar_grant(source: &mut DocumentMut, generated: &DocumentMut, key: &str) {
    let scoped = guarded_filesystem(generated)
        .and_then(|table| table.get(key))
        .is_some_and(Item::is_table_like);
    let scalar = guarded_filesystem(source)
        .and_then(|table| table.get(key))
        .and_then(Item::as_str)
        .map(str::to_owned)
        .filter(|_| scoped);
    let (Some(access), Some(table)) = (scalar, guarded_filesystem_mut(source)) else {
        return;
    };
    let mut kept = toml_edit::Table::new();
    kept.insert(".", toml_edit::value(access));
    table.insert(key, Item::Table(kept));
}

fn guarded_filesystem(document: &DocumentMut) -> Option<&dyn TableLike> {
    document
        .get("permissions")?
        .get("guarded")?
        .get("filesystem")?
        .as_table_like()
}

fn guarded_filesystem_mut(document: &mut DocumentMut) -> Option<&mut dyn TableLike> {
    document
        .get_mut("permissions")?
        .get_mut("guarded")?
        .get_mut("filesystem")?
        .as_table_like_mut()
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

#[cfg(test)]
mod tests;
