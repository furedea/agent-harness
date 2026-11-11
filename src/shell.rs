//! The imperative shell: everything that changes the disk.
//!
//! Computation (`generation`, `provider::spec`, `install_plan`) returns `FileOp` values; this
//! module applies them. The filesystem port in `io` is visible only inside `shell`, so
//! computation cannot reach it even through a wrapper, and clippy bans std's write APIs
//! everywhere else.
mod executor;
#[cfg(test)]
pub(crate) mod fixtures;
pub(crate) mod hook_bundle;
mod io;
pub(crate) mod sync;

pub(crate) use executor::execute;
pub(crate) use io::TemporaryDirectory;
