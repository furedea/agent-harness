//! Keeps a source-declared guarded filesystem access from being weakened by generated entries.
//!
//! The generated fragment is merged over the source `codex/config.toml`, and Codex lets the more
//! specific entry win. So a generated `read` would relax a source `none` on the same path, and
//! would also relax a broader source denial such as `:workspace_roots`, `:root`, or a parent
//! directory. Each generated entry is therefore raised to the strictest source access that
//! covers it.

use toml_edit::{DocumentMut, Item, Table, TableLike, value};

pub(super) fn guarded_filesystem(source: &mut DocumentMut, generated: &mut DocumentMut) {
    let (Some(source), Some(generated)) = (
        guarded_filesystem_mut(source),
        guarded_filesystem_mut(generated),
    ) else {
        return;
    };
    let grants = source_grants(source);
    let paths: Vec<String> = source.iter().map(|(path, _)| path.to_owned()).collect();
    for path in paths {
        reconcile_entry(source, generated, &path);
    }
    raise_covered(generated, &grants);
}

fn guarded_filesystem_mut(document: &mut DocumentMut) -> Option<&mut dyn TableLike> {
    document
        .get_mut("permissions")?
        .get_mut("guarded")?
        .get_mut("filesystem")?
        .as_table_like_mut()
}

/// The part of the filesystem a source access applies to.
#[derive(Debug, PartialEq, Eq)]
enum Coverage {
    Everything,
    Workspace,
    Path(String),
}

impl Coverage {
    fn of_key(key: &str) -> Option<Self> {
        match key {
            ":root" => Some(Self::Everything),
            ":workspace_roots" | ":project_roots" => Some(Self::Workspace),
            _ if key.starts_with('/') || key.starts_with('~') => Some(Self::Path(key.to_owned())),
            _ => None,
        }
    }

    fn covers_absolute(&self, key: &str) -> bool {
        match self {
            Self::Everything => true,
            Self::Workspace => false,
            Self::Path(path) => is_same_or_below(key, path),
        }
    }

    fn covers_workspace(&self) -> bool {
        matches!(self, Self::Everything | Self::Workspace)
    }
}

fn is_same_or_below(key: &str, path: &str) -> bool {
    let parent = path.trim_end_matches('/');
    key == path
        || key
            .strip_prefix(parent)
            .is_some_and(|rest| rest.starts_with('/'))
}

struct Grant {
    coverage: Coverage,
    access: String,
}

fn source_grants(source: &dyn TableLike) -> Vec<Grant> {
    source
        .iter()
        .filter_map(|(key, item)| grant_of(key, item))
        .collect()
}

fn grant_of(key: &str, item: &Item) -> Option<Grant> {
    let coverage = Coverage::of_key(key)?;
    let access = match item.as_table_like() {
        Some(scoped) => scoped.get(".")?.as_str()?,
        None => item.as_str()?,
    };
    Some(Grant {
        coverage,
        access: access.to_owned(),
    })
}

fn raise_covered(generated: &mut dyn TableLike, grants: &[Grant]) {
    let keys: Vec<String> = generated.iter().map(|(key, _)| key.to_owned()).collect();
    for key in keys {
        let scoped = generated.get_mut(&key).and_then(Item::as_table_like_mut);
        match scoped {
            Some(children) => raise_workspace(children, grants),
            None => raise_absolute(generated, &key, grants),
        }
    }
}

fn raise_absolute(generated: &mut dyn TableLike, key: &str, grants: &[Grant]) {
    for grant in grants
        .iter()
        .filter(|grant| grant.coverage.covers_absolute(key))
    {
        raise_access(generated, key, &grant.access);
    }
}

fn raise_workspace(children: &mut dyn TableLike, grants: &[Grant]) {
    let keys: Vec<String> = children.iter().map(|(key, _)| key.to_owned()).collect();
    let covering = grants
        .iter()
        .filter(|grant| grant.coverage.covers_workspace());
    for grant in covering {
        raise_all(children, &keys, &grant.access);
    }
}

fn raise_all(table: &mut dyn TableLike, keys: &[String], access: &str) {
    for key in keys {
        raise_access(table, key, access);
    }
}

fn reconcile_entry(source: &mut dyn TableLike, generated: &mut dyn TableLike, path: &str) {
    let declared = source.get(path).and_then(Item::as_str).map(str::to_owned);
    let Some(access) = declared else {
        reconcile_scoped(source, generated, path);
        return;
    };
    if raise_scoped_children(generated, path, &access) {
        let mut kept = Table::new();
        kept.insert(".", value(access));
        source.insert(path, Item::Table(kept));
    } else {
        raise_access(generated, path, &access);
    }
}

/// Raises each generated entry under `path` to at least `access`; false when `path` is no table.
fn raise_scoped_children(generated: &mut dyn TableLike, path: &str, access: &str) -> bool {
    let Some(scoped) = generated.get_mut(path).and_then(Item::as_table_like_mut) else {
        return false;
    };
    let children: Vec<String> = scoped.iter().map(|(child, _)| child.to_owned()).collect();
    for child in children {
        raise_access(scoped, &child, access);
    }
    true
}

fn reconcile_scoped(source: &mut dyn TableLike, generated: &mut dyn TableLike, path: &str) {
    let (Some(source), Some(generated)) = (
        source.get(path).and_then(Item::as_table_like),
        generated.get_mut(path).and_then(Item::as_table_like_mut),
    ) else {
        return;
    };
    let declared: Vec<(String, String)> = source
        .iter()
        .filter_map(|(child, item)| Some((child.to_owned(), item.as_str()?.to_owned())))
        .collect();
    for (child, access) in declared {
        raise_access(generated, &child, &access);
    }
}

fn raise_access(table: &mut dyn TableLike, path: &str, access: &str) {
    let weaker = table
        .get(path)
        .and_then(Item::as_str)
        .is_some_and(|current| access_rank(current) < access_rank(access));
    if weaker {
        table.insert(path, value(access));
    }
}

/// How restrictive a Codex filesystem access is; `none` is the legacy spelling of `deny`.
fn access_rank(access: &str) -> u8 {
    match access {
        "none" | "deny" => 2,
        "read" => 1,
        _ => 0,
    }
}
