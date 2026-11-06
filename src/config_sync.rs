use std::path::Path;

use anyhow::{Result, bail};

use crate::{
    generation::{devin_config, dsh_config, hermes_config},
    provider::spec::{ConfigSync, ProviderSpec},
};

pub(crate) fn sync(spec: &ProviderSpec, source: &Path, target: &Path) -> Result<()> {
    match spec.config_sync {
        Some(ConfigSync::MergeTopLevelJson) => devin_config::sync_config(source, target),
        Some(ConfigSync::MergeYaml) => hermes_config::sync_config(source, target),
        Some(ConfigSync::RegisterBundle) => dsh_config::sync_config(source, target),
        None => bail!("{} has no managed config to sync", spec.key),
    }
}
