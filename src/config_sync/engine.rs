//! The one sync: apply the declared ownership of a provider's config to a generated config and
//! the installed one.

use std::path::Path;

use anyhow::{Context, Result, bail};

use super::{
    codec::{ConfigDocument, ConfigTree},
    home_paths::expand_command_home_paths,
    installed::{Installed, Synced, Tracked},
};
use crate::provider::ownership::{
    ConfigSync, EntryMatch, KeyScope, Ownership, WhenAbsent, WhenMissing,
};

/// Whether `~/` in the config's command lists is expanded.
#[derive(Clone, Copy, Debug)]
pub(crate) enum HomePaths<'a> {
    Keep,
    Expand(&'a Path),
}

/// A sync of one provider's config: its declared ownership applied to the rendered config.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Merge<'a> {
    sync: &'a ConfigSync,
    generated: &'a ConfigDocument,
    home: HomePaths<'a>,
}

/// What the engine threads through the claims of one merge.
struct Pass<'a, T: ConfigTree> {
    generated: &'a T,
    existing: T,
    known: Option<Tracked>,
    written: Tracked,
}

impl<'a> Merge<'a> {
    pub(crate) fn new(sync: &'a ConfigSync, generated: &'a ConfigDocument) -> Self {
        Self {
            sync,
            generated,
            home: HomePaths::Keep,
        }
    }

    #[must_use]
    pub(crate) fn expanding_home(self, home: &'a Path) -> Self {
        Self {
            home: HomePaths::Expand(home),
            ..self
        }
    }

    /// Whether the sync leaves the config alone because it is missing and the provider makes it.
    pub(crate) fn skips(&self, installed: &Installed) -> bool {
        self.sync.when_missing == WhenMissing::Skip && !installed.exists()
    }

    /// The installed config with the rendered one applied under the declared ownership.
    ///
    /// # Errors
    ///
    /// Returns an error when the two configs disagree in format or shape with what the
    /// ownership declares, for example a list where a mapping must be.
    pub(crate) fn apply(&self, installed: Installed) -> Result<Synced> {
        merge_documents(self, installed)
    }
}

impl<'a, T: ConfigTree> Pass<'a, T> {
    fn new(generated: &'a T, existing: T, known: Option<Tracked>) -> Self {
        Self {
            generated,
            existing,
            known,
            written: Tracked::default(),
        }
    }
}

fn merge_documents(merge: &Merge<'_>, installed: Installed) -> Result<Synced> {
    let (document, known) = installed.into_parts();
    let existing = document.unwrap_or_else(|| ConfigDocument::blank(merge.sync.format));
    match (merge.generated, existing) {
        (ConfigDocument::Json(generated), ConfigDocument::Json(existing)) => {
            run(merge, Pass::new(generated, existing, known))
        }
        (ConfigDocument::Toml(generated), ConfigDocument::Toml(existing)) => {
            run(merge, Pass::new(generated, existing, known))
        }
        (ConfigDocument::Yaml(generated), ConfigDocument::Yaml(existing)) => {
            run(merge, Pass::new(generated, existing, known))
        }
        (generated, existing) => bail!(
            "the generated config is {:?} but the installed one is {:?}",
            generated.format(),
            existing.format(),
        ),
    }
}

fn run<T: ConfigTree>(merge: &Merge<'_>, mut pass: Pass<'_, T>) -> Result<Synced> {
    for claim in merge.sync.ownership {
        apply(*claim, &mut pass)?;
    }
    if let HomePaths::Expand(home) = merge.home {
        expand_home(merge.sync, &mut pass.existing, home)?;
    }
    Ok(Synced {
        config: pass.existing.render()?,
        sidecar: (!pass.written.is_empty())
            .then(|| pass.written.render())
            .transpose()?,
    })
}

fn apply<T: ConfigTree>(claim: Ownership, pass: &mut Pass<'_, T>) -> Result<()> {
    match claim {
        Ownership::Keys { scope, when_absent } => replace_keys(scope, when_absent, pass),
        Ownership::Entries { path, owned } => replace_owned_entries(path, owned, pass),
        Ownership::ListUnion { path } => union_list(path, pass),
        Ownership::MappingEntries { path } => replace_mapping_entries(path, pass),
        Ownership::TrackedKeys {
            path,
            formerly_managed,
            ..
        } => sync_tracked_keys(path, formerly_managed, pass),
    }
}

fn replace_keys<T: ConfigTree>(
    scope: KeyScope,
    when_absent: WhenAbsent,
    pass: &mut Pass<'_, T>,
) -> Result<()> {
    match scope {
        KeyScope::Listed(keys) => keys
            .iter()
            .try_for_each(|key| replace_listed_key(key, when_absent, pass)),
        KeyScope::Every | KeyScope::EveryExcept(_) => pass
            .generated
            .entries()
            .into_iter()
            .filter(|(key, _)| scope.owns(key))
            .try_for_each(|(key, value)| pass.existing.set(&[&key], value)),
    }
}

fn replace_listed_key<T: ConfigTree>(
    key: &str,
    when_absent: WhenAbsent,
    pass: &mut Pass<'_, T>,
) -> Result<()> {
    match (pass.generated.get(&[key]), when_absent) {
        (Some(value), _) => pass.existing.set(&[key], value),
        (None, WhenAbsent::Remove) => {
            pass.existing.remove(&[key]);
            Ok(())
        }
        (None, WhenAbsent::Keep) => Ok(()),
    }
}

fn replace_owned_entries<T: ConfigTree>(
    path: &[&str],
    owned: EntryMatch,
    pass: &mut Pass<'_, T>,
) -> Result<()> {
    let Some(section) = pass.generated.get(path) else {
        return Ok(());
    };
    let lists = T::mapping(&section)
        .with_context(|| format!("generated {} must be a mapping", path.join(".")))?;
    for (key, generated) in lists {
        let list_path = [path, &[key.as_str()]].concat();
        let installed = pass.existing.get(&list_path);
        let list = replaced_list::<T>(owned, &generated, installed.as_ref())
            .with_context(|| format!("failed to sync {}", list_path.join(".")))?;
        pass.existing.set(&list_path, list)?;
    }
    Ok(())
}

/// The installed list without the entries the harness owns, followed by the generated ones.
fn replaced_list<T: ConfigTree>(
    owned: EntryMatch,
    generated: &T::Value,
    installed: Option<&T::Value>,
) -> Result<T::Value> {
    let generated = T::sequence(generated).context("the generated value must be a list")?;
    let installed = installed
        .map(|list| T::sequence(list).context("the installed value must be a list"))
        .transpose()?
        .unwrap_or_default();
    let kept = installed
        .into_iter()
        .filter(|entry| !is_owned::<T>(owned, entry));
    Ok(T::from_sequence(kept.chain(generated).collect()))
}

fn is_owned<T: ConfigTree>(owned: EntryMatch, entry: &T::Value) -> bool {
    T::text(entry).is_some_and(|text| owned.matches_text(&text))
}

fn union_list<T: ConfigTree>(path: &[&str], pass: &mut Pass<'_, T>) -> Result<()> {
    let Some(generated) = pass.generated.get(path) else {
        return Ok(());
    };
    let generated = T::sequence(&generated)
        .with_context(|| format!("generated {} must be a list", path.join(".")))?;
    let mut entries = installed_list(path, &pass.existing)?;
    for entry in generated {
        push_unique::<T>(&mut entries, entry);
    }
    pass.existing.set(path, T::from_sequence(entries))
}

fn installed_list<T: ConfigTree>(path: &[&str], existing: &T) -> Result<Vec<T::Value>> {
    let Some(installed) = existing.get(path) else {
        return Ok(Vec::new());
    };
    T::sequence(&installed).with_context(|| format!("installed {} must be a list", path.join(".")))
}

fn push_unique<T: ConfigTree>(entries: &mut Vec<T::Value>, entry: T::Value) {
    if !entries.iter().any(|known| T::same(known, &entry)) {
        entries.push(entry);
    }
}

fn replace_mapping_entries<T: ConfigTree>(path: &[&str], pass: &mut Pass<'_, T>) -> Result<()> {
    for (key, value) in generated_entries(path, pass.generated)? {
        pass.existing
            .set(&[path, &[key.as_str()]].concat(), value)?;
    }
    Ok(())
}

/// The entries of the mapping at `path` in the rendered config; a missing mapping has none.
fn generated_entries<T: ConfigTree>(
    path: &[&str],
    generated: &T,
) -> Result<Vec<(String, T::Value)>> {
    let Some(section) = generated.get(path) else {
        return Ok(Vec::new());
    };
    T::mapping(&section).with_context(|| format!("generated {} must be a mapping", path.join(".")))
}

fn sync_tracked_keys<T: ConfigTree>(
    path: &[&str],
    formerly_managed: &[&str],
    pass: &mut Pass<'_, T>,
) -> Result<()> {
    let generated = pass
        .generated
        .get(path)
        .and_then(|section| T::mapping(&section))
        .unwrap_or_default();
    let name = path.join("_");
    let previous = Tracked::previous(pass.known.as_ref(), &name, formerly_managed);
    for stale in previous
        .iter()
        .filter(|key| !generated.iter().any(|(name, _)| name == *key))
    {
        pass.existing.remove(&[path, &[stale.as_str()]].concat());
    }
    let written = generated.iter().map(|(key, _)| key.clone()).collect();
    for (key, value) in generated {
        pass.existing
            .set(&[path, &[key.as_str()]].concat(), value)?;
    }
    pass.written.record(name, written);
    Ok(())
}

fn expand_home<T: ConfigTree>(sync: &ConfigSync, existing: &mut T, home: &Path) -> Result<()> {
    let home = home.to_string_lossy();
    sync.home_command_lists
        .iter()
        .try_for_each(|path| expand_command_list(existing, path, &home))
}

fn expand_command_list<T: ConfigTree>(existing: &mut T, path: &[&str], home: &str) -> Result<()> {
    let Some(commands) = existing.get(path).as_ref().and_then(T::sequence) else {
        return Ok(());
    };
    let expanded = commands
        .into_iter()
        .map(|command| expand_command::<T>(command, home))
        .collect();
    existing.set(path, T::from_sequence(expanded))
}

fn expand_command<T: ConfigTree>(item: T::Value, home: &str) -> T::Value {
    match T::text(&item) {
        Some(command) => T::from_text(expand_command_home_paths(&command, home)),
        None => item,
    }
}

#[cfg(test)]
mod tests;
