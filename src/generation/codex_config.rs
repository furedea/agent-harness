use std::path::Path;

use anyhow::{Context, Result};
use toml_edit::{DocumentMut, Item};

use crate::{
    generation::{external_hooks::ExternalHookBundle, io, protection},
    layout::SourceLayout,
};

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

pub(crate) fn write_config_source(
    source: &Path,
    out: &Path,
    external_hooks: &[ExternalHookBundle],
) -> Result<()> {
    io::write_file(out, &config_source_content(source, external_hooks)?)
}

pub(crate) fn sync_managed_config(source_path: &Path, target_path: &Path) -> Result<()> {
    let source = read_toml_document(source_path)?;
    sync_managed_document(source, target_path)
}

pub(crate) fn sync_generated_config(
    source: &Path,
    target_path: &Path,
    external_hooks: &[ExternalHookBundle],
) -> Result<()> {
    let source = config_source_content(source, external_hooks)?
        .parse::<DocumentMut>()
        .context("failed to parse generated Codex config source")?;
    sync_managed_document(source, target_path)
}

fn config_source_content(source: &Path, external_hooks: &[ExternalHookBundle]) -> Result<String> {
    let base_path = SourceLayout::new(source).codex_config();
    let base = std::fs::read_to_string(&base_path)
        .with_context(|| format!("failed to read TOML file {}", base_path.display()))?;
    let mut document = format!(
        "{}\n{}",
        base.trim_end(),
        protection::codex_config_fragment(source, external_hooks)?
    )
    .parse::<DocumentMut>()
    .context("failed to parse generated Codex config")?;
    for bundle in external_hooks {
        let Some(path) = bundle.codex_config_path()? else {
            continue;
        };
        let generated = read_toml_document(&path)?;
        merge_external_features(&mut document, &generated, &path)?;
    }
    Ok(document.to_string())
}

fn merge_external_features(
    target: &mut DocumentMut,
    source: &DocumentMut,
    path: &Path,
) -> Result<()> {
    for (key, item) in source.iter() {
        if key != "features" {
            anyhow::bail!(
                "external hook config {} contains unsupported top-level key: {key}",
                path.display(),
            );
        }
        merge_item(&mut target[key], item);
    }
    Ok(())
}

fn merge_item(target: &mut Item, source: &Item) {
    match (target.as_table_mut(), source.as_table()) {
        (Some(target), Some(source)) => {
            for (key, item) in source {
                merge_item(&mut target[key], item);
            }
        }
        _ => *target = source.clone(),
    }
}

fn sync_managed_document(source: DocumentMut, target_path: &Path) -> Result<()> {
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

    #[test]
    fn write_config_source_appends_guarded_filesystem_fragment() -> Result<()> {
        let root = test_root("write_config_source_appends_guarded_filesystem_fragment")?;
        write_minimal_source(&root)?;
        let out = root.join("config.toml");

        write_config_source(&root, &out, &[])?;

        let content = std::fs::read_to_string(&out)?;
        assert!(content.contains("model = \"gpt-5.5\""));
        assert!(content.contains("[permissions.guarded.filesystem]"));
        assert!(content.contains("\"~/.codex/hooks/adapt.sh\" = \"read\""));

        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    fn test_root(name: &str) -> Result<std::path::PathBuf> {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let root = std::env::temp_dir().join(format!("agent-harness-{name}-{nanos}"));
        std::fs::create_dir_all(&root)?;
        Ok(root)
    }

    fn write_minimal_source(source: &Path) -> Result<()> {
        write_file(&source.join("codex/config.toml"), "model = \"gpt-5.5\"\n")?;
        write_file(&source.join("agents/AGENTS.md"), "agent instructions\n")?;
        write_file(&source.join("agents/hooks/guard.sh"), "#!/bin/bash\n")?;
        write_file(&source.join("codex/hooks/adapt.sh"), "#!/bin/bash\n")?;
        Ok(())
    }

    fn write_file(path: &Path, content: &str) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, content)?;
        Ok(())
    }
}
