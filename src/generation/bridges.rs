use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::fs_ops;

const HERMES_PLUGIN_ASSETS: [(&str, &str); 2] = [
    ("plugin.yaml", include_str!("assets/hermes/plugin.yaml")),
    ("__init__.py", include_str!("assets/hermes/__init__.py")),
];
const PI_HOOK_BRIDGE: &str = include_str!("assets/pi/hook_bridge.ts");

pub(crate) fn write_hermes_plugin(dir: &Path) -> Result<()> {
    for (name, content) in HERMES_PLUGIN_ASSETS {
        fs_ops::write_file_atomically(&dir.join(name), content.as_bytes())?;
    }
    Ok(())
}

pub(crate) fn hermes_plugin_files(dir: &Path) -> Vec<PathBuf> {
    HERMES_PLUGIN_ASSETS
        .iter()
        .map(|(name, _)| dir.join(name))
        .collect()
}

pub(crate) fn write_pi_bridge(path: &Path) -> Result<()> {
    fs_ops::write_file_atomically(path, PI_HOOK_BRIDGE.as_bytes())
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

    fn test_root(name: &str) -> Result<PathBuf> {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let root = std::env::temp_dir().join(format!("agent-harness-{name}-{nanos}"));
        std::fs::create_dir_all(&root)?;
        Ok(root)
    }
}
