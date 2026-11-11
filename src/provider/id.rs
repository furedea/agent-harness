// `deny`, not `forbid`: clap's derive emits `allow(clippy::style)`, which `forbid` rejects.
#![deny(clippy::disallowed_methods, clippy::disallowed_types)]

/// Which provider a command or declaration names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, clap::ValueEnum)]
pub(crate) enum ProviderId {
    Claude,
    Codex,
    Devin,
    Hermes,
    Pi,
    Dsh,
}
