//! Detects guarded filesystem entries that disagree about the same path.
//!
//! Codex applies the more specific entry, so a looser entry under a stricter one, such as
//! `secrets/key = "read"` under `secrets/** = "none"`, reopens what the stricter one denies. The
//! harness does not guess which of the two the author meant: it fails and names both.
//!
//! Only a pair with a generated entry is checked; the source's own entries are its author's.
//! Two globs are never compared with each other, because the entries a denial could be wrongly
//! reopened by are concrete paths. A glob that is looser than another glob is not detected.

use std::path::Path;

use anyhow::{Result, bail};
use toml_edit::{DocumentMut, Item, TableLike};

use crate::generation::guarded_path::{
    Access, GuardedEntry, GuardedPath, conflicts, is_absolute_key,
};

const WORKSPACE_KEYS: [&str; 2] = [":workspace_roots", ":project_roots"];

/// The source `codex/config.toml` the generated fragment is merged over.
pub(crate) struct SourceConfig<'a> {
    pub(crate) path: &'a Path,
    pub(crate) document: &'a DocumentMut,
}

/// # Errors
///
/// Returns an error naming both entries, their accesses and where each is declared when one
/// covers a looser one and at least one of them is generated.
pub(crate) fn check(generated: &DocumentMut, source: Option<SourceConfig<'_>>) -> Result<()> {
    let mut entries = located_entries(generated, &Origin::Generated)?;
    if let Some(source) = source {
        let origin = Origin::Source(source.path.display().to_string());
        entries.extend(located_entries(source.document, &origin)?);
    }
    let guarded: Vec<GuardedEntry> = entries.iter().map(|entry| entry.entry.clone()).collect();
    let conflict = conflicts(&guarded)
        .into_iter()
        .find(|conflict| !both_from_source(&entries, conflict.stricter, conflict.looser));
    let Some(conflict) = conflict else {
        return Ok(());
    };
    let (Some(stricter), Some(looser)) =
        (entries.get(conflict.stricter), entries.get(conflict.looser))
    else {
        return Ok(());
    };
    bail!(
        "guarded filesystem conflict: {} covers {}, which is looser; Codex applies the more \
         specific entry, so the looser one would reopen the path. Change one of them",
        stricter.describe(),
        looser.describe()
    )
}

fn both_from_source(entries: &[Located], first: usize, second: usize) -> bool {
    let from_source = |index: usize| {
        entries
            .get(index)
            .is_some_and(|entry| matches!(entry.origin, Origin::Source(_)))
    };
    from_source(first) && from_source(second)
}

#[derive(Clone)]
enum Origin {
    Generated,
    Source(String),
}

struct Located {
    label: String,
    access: String,
    origin: Origin,
    entry: GuardedEntry,
}

impl Located {
    fn describe(&self) -> String {
        let place = match &self.origin {
            Origin::Generated => "generated".to_owned(),
            Origin::Source(path) => path.clone(),
        };
        format!("{} = \"{}\" ({place})", self.label, self.access)
    }
}

fn located_entries(document: &DocumentMut, origin: &Origin) -> Result<Vec<Located>> {
    let Some(table) = guarded_filesystem(document) else {
        return Ok(Vec::new());
    };
    let mut located = Vec::new();
    for (key, item) in table.iter() {
        match item.as_table_like() {
            Some(scoped) => located.extend(scoped_entries(key, scoped, origin)?),
            None => located.extend(plain_entry(key, item, origin)?),
        }
    }
    Ok(located)
}

fn guarded_filesystem(document: &DocumentMut) -> Option<&dyn TableLike> {
    document
        .get("permissions")?
        .get("guarded")?
        .get("filesystem")?
        .as_table_like()
}

fn plain_entry(key: &str, item: &Item, origin: &Origin) -> Result<Option<Located>> {
    let Some(base) = base_path(key)? else {
        return Ok(None);
    };
    Ok(entry_of(base, item, Site::new(key, origin)))
}

fn scoped_entries(scope: &str, table: &dyn TableLike, origin: &Origin) -> Result<Vec<Located>> {
    let Some(base) = base_path(scope)? else {
        return Ok(Vec::new());
    };
    let mut located = Vec::new();
    for (child, item) in table.iter() {
        let site = Site::new(&format!("{scope} / {child}"), origin);
        located.extend(entry_of(base.join(child)?, item, site));
    }
    Ok(located)
}

/// Where an entry is declared: its key, shown in errors, and the document it came from.
struct Site<'a> {
    label: String,
    origin: &'a Origin,
}

impl<'a> Site<'a> {
    fn new(label: &str, origin: &'a Origin) -> Self {
        Self {
            label: label.to_owned(),
            origin,
        }
    }
}

fn entry_of(path: GuardedPath, item: &Item, site: Site<'_>) -> Option<Located> {
    let text = item.as_str()?;
    let entry = GuardedEntry::new(path, Access::parse(text)?)?;
    Some(Located {
        label: site.label,
        access: text.to_owned(),
        origin: site.origin.clone(),
        entry,
    })
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
