#![forbid(clippy::disallowed_methods, clippy::disallowed_types)]
#![expect(
    dead_code,
    reason = "the engine that uses the codecs follows in the next commit"
)]
//! Config sync: the parts of a provider's config the harness owns, applied to what is installed.
//!
//! Format codecs ([`codec`]) turn text into documents and back. The ownership each provider
//! declares says which parts of its config the harness writes.

pub(crate) mod codec;
mod engine;
mod home_paths;
pub(crate) mod installed;

#[expect(
    unused_imports,
    reason = "the providers migrate to the engine in the next commit"
)]
pub(crate) use engine::Merge;
