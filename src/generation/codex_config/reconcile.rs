//! Keeps a source-declared guarded filesystem access from being weakened by generated entries.
//!
//! The generated fragment is merged over the source `codex/config.toml`, and Codex lets the more
//! specific entry win. Every entry of both documents is therefore read into the guarded path
//! model, and each looser one is raised to the strictest entry that covers it, wherever that
//! entry is declared.

use anyhow::Result;
use toml_edit::{DocumentMut, Item, Table, TableLike, value};

use crate::generation::guarded_path::{
    Access, GuardedEntry, GuardedPath, is_absolute_key, raised_by,
};

pub(super) fn guarded_filesystem(
    source: &mut DocumentMut,
    generated: &mut DocumentMut,
) -> Result<()> {
    let (Some(source), Some(generated)) = (
        guarded_filesystem_mut(source),
        guarded_filesystem_mut(generated),
    ) else {
        return Ok(());
    };
    keep_scalar_workspace_grants(source, generated);
    let declared = located_entries(source)?;
    let produced = located_entries(generated)?;
    let declared_entries = entries_of(&declared);
    let produced_entries = entries_of(&produced);
    let all: Vec<GuardedEntry> = declared_entries
        .iter()
        .chain(&produced_entries)
        .cloned()
        .collect();
    let denials: Vec<GuardedEntry> = produced_entries
        .iter()
        .filter(|entry| entry.is_denial())
        .cloned()
        .collect();
    // A generated entry must not weaken anything, but a source entry changes only when a
    // generated denial covers it: the source may deliberately open a path under a broader one,
    // such as `:workspace_roots = "write"` under `:root = "read"`.
    raise(source, &declared, &raised_by(&declared_entries, &denials));
    raise(generated, &produced, &raised_by(&produced_entries, &all));
    Ok(())
}

fn entries_of(located: &[Located]) -> Vec<GuardedEntry> {
    located
        .iter()
        .map(|located| located.entry.clone())
        .collect()
}

fn guarded_filesystem_mut(document: &mut DocumentMut) -> Option<&mut dyn TableLike> {
    document
        .get_mut("permissions")?
        .get_mut("guarded")?
        .get_mut("filesystem")?
        .as_table_like_mut()
}

const WORKSPACE_KEYS: [&str; 2] = [":workspace_roots", ":project_roots"];

/// A scalar source grant on the workspace becomes a `"."` entry when the generated fragment
/// scopes entries under the same key, because a merge would replace the scalar with the table.
fn keep_scalar_workspace_grants(source: &mut dyn TableLike, generated: &dyn TableLike) {
    for key in WORKSPACE_KEYS {
        let scoped = generated.get(key).is_some_and(Item::is_table_like);
        let scalar = source.get(key).and_then(Item::as_str).map(str::to_owned);
        scalar
            .filter(|_| scoped)
            .into_iter()
            .for_each(|access| scope_scalar(source, key, access));
    }
}

fn scope_scalar(source: &mut dyn TableLike, key: &str, access: String) {
    let mut kept = Table::new();
    kept.insert(".", value(access));
    source.insert(key, Item::Table(kept));
}

/// Where an entry's access is written: directly in the table, or in a scoped table under `scope`.
struct Slot {
    scope: Option<String>,
    key: String,
}

struct Located {
    slot: Slot,
    entry: GuardedEntry,
}

fn located_entries(table: &dyn TableLike) -> Result<Vec<Located>> {
    let mut located = Vec::new();
    for (key, item) in table.iter() {
        match item.as_table_like() {
            Some(scoped) => located.extend(scoped_entries(key, scoped)?),
            None => located.extend(plain_entry(key, item)?),
        }
    }
    Ok(located)
}

fn plain_entry(key: &str, item: &Item) -> Result<Option<Located>> {
    let Some(base) = base_path(key)? else {
        return Ok(None);
    };
    Ok(entry_of(base, item).map(|entry| Located {
        slot: Slot {
            scope: None,
            key: key.to_owned(),
        },
        entry,
    }))
}

fn scoped_entries(scope: &str, table: &dyn TableLike) -> Result<Vec<Located>> {
    let Some(base) = base_path(scope)? else {
        return Ok(Vec::new());
    };
    let mut located = Vec::new();
    for (child, item) in table.iter() {
        let entry = entry_of(base.join(child)?, item);
        located.extend(entry.map(|entry| Located {
            slot: Slot {
                scope: Some(scope.to_owned()),
                key: child.to_owned(),
            },
            entry,
        }));
    }
    Ok(located)
}

fn entry_of(path: GuardedPath, item: &Item) -> Option<GuardedEntry> {
    let access = Access::parse(item.as_str()?)?;
    GuardedEntry::new(path, access)
}

/// The path a table key names; `None` for a key the model does not cover, such as `:tmpdir`.
fn base_path(key: &str) -> Result<Option<GuardedPath>> {
    match key {
        ":root" => Ok(Some(GuardedPath::everything())),
        _ if WORKSPACE_KEYS.contains(&key) => Ok(Some(GuardedPath::workspace())),
        ":slash_tmp" => GuardedPath::absolute("/tmp").map(Some),
        _ if is_absolute_key(key) => GuardedPath::absolute(key).map(Some),
        _ => Ok(None),
    }
}

fn raise(table: &mut dyn TableLike, located: &[Located], required: &[Access]) {
    let raised = located
        .iter()
        .zip(required)
        .filter(|(located, access)| **access > located.entry.access());
    for (located, access) in raised {
        write_access(table, &located.slot, *access);
    }
}

fn write_access(table: &mut dyn TableLike, slot: &Slot, access: Access) {
    let target = match &slot.scope {
        Some(scope) => table.get_mut(scope).and_then(Item::as_table_like_mut),
        None => Some(table),
    };
    if let Some(target) = target {
        target.insert(&slot.key, value(access.as_str()));
    }
}
