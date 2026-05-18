use std::path::Path;

use anyhow::{Context, Result};
use toml_edit::DocumentMut;

const MANAGED_KEYS: &[&str] = &[
    "model",
    "model_reasoning_effort",
    "personality",
    "approval_policy",
    "sandbox_mode",
    "approvals_reviewer",
    "notice",
    "tui",
    "plugins",
    "features",
    "default_permissions",
    "permissions",
];

pub fn sync_managed_config(source_path: &Path, target_path: &Path) -> Result<()> {
    let source = read_toml_document(source_path)?;
    let mut target = if target_path.exists() {
        read_toml_document(target_path)?
    } else {
        DocumentMut::new()
    };

    for key in MANAGED_KEYS {
        match source.get(key) {
            Some(item) => target[key] = item.clone(),
            None => {
                target.remove(key);
            }
        }
    }

    write_toml_document(target_path, &target)
}

fn read_toml_document(path: &Path) -> Result<DocumentMut> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read TOML file {}", path.display()))?;
    content
        .parse::<DocumentMut>()
        .with_context(|| format!("failed to parse TOML file {}", path.display()))
}

fn write_toml_document(path: &Path, document: &DocumentMut) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory {}", parent.display()))?;
    }

    let temp_path = path.with_extension("tmp");
    std::fs::write(&temp_path, document.to_string()).with_context(|| {
        format!(
            "failed to write temporary TOML file {}",
            temp_path.display()
        )
    })?;
    std::fs::rename(&temp_path, path)
        .with_context(|| format!("failed to replace TOML file {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_managed_config_preserves_codex_owned_project_state() -> Result<()> {
        let source = r#"
model = "gpt-5.5"

[features]
hooks = true
"#;
        let target = r#"
model = "old"

[projects."/work"]
trust_level = "trusted"
"#;

        let mut merged = target.parse::<DocumentMut>()?;
        let source = source.parse::<DocumentMut>()?;
        for key in MANAGED_KEYS {
            match source.get(key) {
                Some(item) => merged[key] = item.clone(),
                None => {
                    merged.remove(key);
                }
            }
        }

        assert_eq!(merged["model"].as_str(), Some("gpt-5.5"));
        assert_eq!(merged["features"]["hooks"].as_bool(), Some(true));
        assert_eq!(
            merged["projects"]["/work"]["trust_level"].as_str(),
            Some("trusted"),
        );

        Ok(())
    }
}
