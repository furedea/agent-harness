//! Characterization of what each provider's sync leaves in the installed config.
//!
//! These pin the observable result of `sync` per provider, so the ownership each provider
//! declares can be checked against what the harness has always done.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use serde_json::Value;

use super::{SyncPaths, sync};
use crate::provider::spec::ProviderId;
use crate::shell::fixtures::{create_dir_all, remove_dir_all, write};

pub(super) struct Sandbox {
    pub(super) root: PathBuf,
}

impl Sandbox {
    pub(super) fn new(name: &str) -> Result<Self> {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let root = std::env::temp_dir().join(format!("agent-harness-sync-{name}-{nanos}"));
        create_dir_all(&root)?;
        Ok(Self { root })
    }

    pub(super) fn path(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }

    pub(super) fn put(&self, name: &str, content: &str) -> Result<PathBuf> {
        let path = self.path(name);
        if let Some(parent) = path.parent() {
            create_dir_all(parent)?;
        }
        write(&path, content)?;
        Ok(path)
    }

    pub(super) fn put_json(&self, name: &str, value: &Value) -> Result<PathBuf> {
        self.put(name, &serde_json::to_string(value)?)
    }

    pub(super) fn sync(&self, provider: ProviderId, source: &Path, target: &Path) -> Result<()> {
        sync(provider.spec(), &SyncPaths { source, target }, &self.root)
    }

    pub(super) fn json(&self, name: &str) -> Result<Value> {
        Ok(serde_json::from_str(&std::fs::read_to_string(
            self.path(name),
        )?)?)
    }

    pub(super) fn text(&self, name: &str) -> Result<String> {
        Ok(std::fs::read_to_string(self.path(name))?)
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = remove_dir_all(&self.root);
    }
}

mod claude;
mod codex;
mod devin;
mod other;
