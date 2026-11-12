use std::path::Path;

use anyhow::{Context, Result};
use toml_edit::{DocumentMut, Item, Table, TableLike, value};

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
    reconcile_guarded_filesystem(&mut document, &mut protection);
    for (key, item) in protection.iter() {
        merge_item(document.entry(key).or_insert(Item::None), item);
    }
    for bundle in &context.source.hook_bundles {
        merge_external_config(&mut document, bundle)?;
    }
    Ok(document.to_string())
}

/// Keeps the source's access on a path the generated fragment also names, unless the fragment
/// is stricter, and keeps a scalar `:workspace_roots` grant when the fragment scopes entries
/// under it. A plain merge would let the generated value replace both.
fn reconcile_guarded_filesystem(source: &mut DocumentMut, generated: &mut DocumentMut) {
    for declared in declared_accesses(source) {
        reconcile_path(source, generated, &declared);
    }
}

struct DeclaredAccess {
    path: String,
    access: String,
}

fn declared_accesses(document: &DocumentMut) -> Vec<DeclaredAccess> {
    let Some(filesystem) = guarded_filesystem(document) else {
        return Vec::new();
    };
    filesystem
        .iter()
        .filter_map(|(path, item)| {
            Some(DeclaredAccess {
                path: path.to_owned(),
                access: item.as_str()?.to_owned(),
            })
        })
        .collect()
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

fn reconcile_path(
    source: &mut DocumentMut,
    generated: &mut DocumentMut,
    declared: &DeclaredAccess,
) {
    let Some(generated) = guarded_filesystem_mut(generated) else {
        return;
    };
    match generated.get(&declared.path).map(Item::as_str) {
        Some(Some(access)) if access_rank(access) < access_rank(&declared.access) => {
            generated.insert(&declared.path, value(declared.access.as_str()));
        }
        Some(None) => scope_declared_grant(source, declared),
        _ => {}
    }
}

fn scope_declared_grant(source: &mut DocumentMut, declared: &DeclaredAccess) {
    let Some(filesystem) = guarded_filesystem_mut(source) else {
        return;
    };
    let mut scoped = Table::new();
    scoped.insert(".", value(declared.access.as_str()));
    filesystem.insert(&declared.path, Item::Table(scoped));
}

/// How restrictive a Codex filesystem access is; `none` is the legacy spelling of `deny`.
fn access_rank(access: &str) -> u8 {
    match access {
        "none" | "deny" => 2,
        "read" => 1,
        _ => 0,
    }
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
