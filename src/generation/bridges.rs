use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::fs_ops;

use crate::provider::spec::{ProviderId, ProviderSpec};

pub(crate) fn write_hermes_plugin(dir: &Path) -> Result<()> {
    write_assets(dir, ProviderId::Hermes.spec())
}

pub(crate) fn hermes_plugin_files(dir: &Path) -> Vec<PathBuf> {
    ProviderId::Hermes
        .spec()
        .assets
        .iter()
        .map(|asset| dir.join(asset.name))
        .collect()
}

pub(crate) fn write_pi_bridge(path: &Path) -> Result<()> {
    fs_ops::write_file_atomically(path, ProviderId::Pi.spec().assets[0].content.as_bytes())
}

pub(crate) fn write_dsh_bridge(dir: &Path) -> Result<()> {
    write_assets(dir, ProviderId::Dsh.spec())
}

fn write_assets(dir: &Path, spec: &ProviderSpec) -> Result<()> {
    for asset in spec.assets {
        fs_ops::write_file_atomically(&dir.join(asset.name), asset.content.as_bytes())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn write_hermes_plugin_emits_the_manifest_and_register_entrypoint() -> Result<()> {
        let root = test_root("write_hermes_plugin")?;
        let dir = root.join("agent-harness-hooks");

        write_hermes_plugin(&dir)?;

        let manifest = std::fs::read_to_string(dir.join("plugin.yaml"))?;
        assert!(manifest.contains("name: agent-harness-hooks"));
        let init = std::fs::read_to_string(dir.join("__init__.py"))?;
        assert!(init.contains("def register(ctx"));
        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn write_pi_bridge_emits_the_extension_entrypoint() -> Result<()> {
        let root = test_root("write_pi_bridge")?;
        let path = root.join("extensions/hook_bridge.ts");

        write_pi_bridge(&path)?;

        let bridge = std::fs::read_to_string(&path)?;
        assert!(bridge.contains("export default function (pi: ExtensionAPI)"));
        assert!(bridge.contains("pi.on(\"tool_call\""));
        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn pi_bridge_runs_the_adapter_with_an_isolated_interpreter() {
        assert!(
            ProviderId::Pi.spec().assets[0]
                .content
                .contains(r#"spawn("python3", ["-I", "-B", ADAPTER, "dispatch", event]"#)
        );
        assert!(
            !ProviderId::Pi.spec().assets[0]
                .content
                .contains(r#"spawn("python3", [ADAPTER"#)
        );
    }

    fn test_root(name: &str) -> Result<PathBuf> {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let root = std::env::temp_dir().join(format!("agent-harness-{name}-{nanos}"));
        std::fs::create_dir_all(&root)?;
        Ok(root)
    }
}
