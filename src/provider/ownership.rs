#![forbid(clippy::disallowed_methods, clippy::disallowed_types)]
#![expect(
    dead_code,
    reason = "the registry declares its ownership in the next commit"
)]
//! What a provider declares about the config the harness keeps in step with the rendered one.
//!
//! The declaration has two independent parts. The format says how the text is read and written;
//! the ownership says which parts of the config the harness writes. Everything outside the
//! declared ownership belongs to the user or the provider and is never touched. One engine in
//! `config_sync` applies any declaration to any format.

/// The text format of a provider's config file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ConfigFormat {
    Json,
    /// Edited in place, so comments and layout the user wrote survive a sync.
    Toml,
    Yaml,
}

/// What to do when the installed config does not exist yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WhenMissing {
    /// Start from an empty config.
    Create,
    /// The provider creates the file itself; leave it for it to initialize.
    Skip,
}

/// Which top-level keys of the rendered config the harness owns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum KeyScope {
    /// Every key the rendered config has.
    Every,
    /// Every key but these, which another claim owns.
    EveryExcept(&'static [&'static str]),
    /// Only these keys, whatever else the rendered config holds.
    Listed(&'static [&'static str]),
}

impl KeyScope {
    pub(crate) fn is_listed(self) -> bool {
        matches!(self, Self::Listed(_))
    }

    pub(crate) fn owns(self, key: &str) -> bool {
        match self {
            Self::Every => true,
            Self::EveryExcept(others) => !others.contains(&key),
            Self::Listed(keys) => keys.contains(&key),
        }
    }
}

/// What happens to an owned key that the rendered config no longer has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WhenAbsent {
    /// The installed value stays.
    Keep,
    /// The installed value is removed, so dropping a key from the source drops it from the config.
    Remove,
}

/// Which entries of a list the harness owns, and so replaces on every sync.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EntryMatch {
    /// String entries that start with this prefix.
    StringPrefix(&'static str),
}

impl EntryMatch {
    pub(crate) fn matches_text(self, text: &str) -> bool {
        match self {
            Self::StringPrefix(prefix) => text.starts_with(prefix),
        }
    }
}

/// One part of a config the harness owns, addressed by the keys that lead to it.
///
/// Claims are applied in order. They must not overlap: a part owned by one claim is left out of
/// the others, as [`KeyScope::EveryExcept`] does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Ownership {
    /// Top-level keys, each replaced by the rendered value as a whole.
    Keys {
        scope: KeyScope,
        when_absent: WhenAbsent,
    },
    /// The lists under the mapping at `path`: the entries that match are replaced by the rendered
    /// ones, and every other entry stays.
    Entries {
        path: &'static [&'static str],
        owned: EntryMatch,
    },
    /// The list at `path` keeps what is installed and gains the rendered entries it lacks.
    ListUnion { path: &'static [&'static str] },
    /// The entries of the mapping at `path`, each replaced individually; other entries stay.
    MappingEntries { path: &'static [&'static str] },
    /// The entries of the mapping at `path` that the rendered config has, plus those the
    /// previous sync wrote and the rendered config dropped. A sidecar file next to the config
    /// remembers which were written, so entries the user added are never removed.
    TrackedKeys {
        path: &'static [&'static str],
        sidecar: &'static str,
        /// Entries the harness managed before the sidecar existed, used while there is none.
        formerly_managed: &'static [&'static str],
    },
}

/// How installation keeps a provider's mutable config in step with the rendered one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ConfigSync {
    pub(crate) format: ConfigFormat,
    /// The parts the harness owns, applied in order.
    pub(crate) ownership: &'static [Ownership],
    pub(crate) when_missing: WhenMissing,
    /// Lists of shell commands whose leading `~/` words become the home directory on a sync.
    pub(crate) home_command_lists: &'static [&'static [&'static str]],
}

impl ConfigSync {
    /// The file next to the config that records what the harness wrote, if any claim needs one.
    pub(crate) fn sidecar_file(&self) -> Option<&'static str> {
        self.ownership.iter().find_map(|claim| match claim {
            Ownership::TrackedKeys { sidecar, .. } => Some(*sidecar),
            _ => None,
        })
    }

    /// Whether every key of the rendered config replaces the installed one.
    pub(crate) fn replaces_every_key(&self) -> bool {
        self.key_scopes().any(|scope| scope == KeyScope::Every)
    }

    /// Whether some claim owns top-level keys by listing them.
    pub(crate) fn owns_listed_keys(&self) -> bool {
        self.key_scopes().any(KeyScope::is_listed)
    }

    fn key_scopes(&self) -> impl Iterator<Item = KeyScope> {
        self.ownership.iter().filter_map(Ownership::key_scope)
    }
}

impl Ownership {
    fn key_scope(&self) -> Option<KeyScope> {
        match self {
            Self::Keys { scope, .. } => Some(*scope),
            _ => None,
        }
    }
}
