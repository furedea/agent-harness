#![forbid(clippy::disallowed_methods, clippy::disallowed_types)]

/// The text format of a provider's config file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ConfigFormat {
    Json,
    /// Edited in place, so comments and layout the user wrote survive a sync.
    Toml,
    Yaml,
}
