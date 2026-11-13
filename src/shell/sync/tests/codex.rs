use anyhow::Result;

use super::Sandbox;
use crate::provider::spec::ProviderId;

#[test]
fn codex_replaces_managed_keys_removes_absent_ones_and_keeps_the_rest() -> Result<()> {
    let sandbox = Sandbox::new("codex-keys")?;
    let source = sandbox.put(
        "generated.toml",
        "model = \"gpt-5.5\"\n\n[features]\nhooks = true\n",
    )?;
    let target = sandbox.put(
        "config.toml",
        "model = \"old\"\npersonality = \"terse\"\n\n[projects.\"/work\"]\ntrust_level = \"trusted\"\n",
    )?;

    sandbox.sync(
        ProviderId::Codex,
        &super::SyncPaths {
            source: &source,
            target: &target,
        },
    )?;

    let merged: toml_edit::DocumentMut = sandbox.text("config.toml")?.parse()?;
    assert_eq!(merged["model"].as_str(), Some("gpt-5.5"));
    assert_eq!(merged["features"]["hooks"].as_bool(), Some(true));
    assert!(merged.get("personality").is_none());
    assert_eq!(
        merged["projects"]["/work"]["trust_level"].as_str(),
        Some("trusted"),
    );
    Ok(())
}

#[test]
fn codex_preserves_the_formatting_of_what_it_keeps() -> Result<()> {
    let sandbox = Sandbox::new("codex-format")?;
    let source = sandbox.put("generated.toml", "model = \"gpt-5.5\"\n")?;
    let target = sandbox.put(
        "config.toml",
        "# my notes\n[projects.\"/work\"]\ntrust_level   = \"trusted\" # keep\n",
    )?;

    sandbox.sync(
        ProviderId::Codex,
        &super::SyncPaths {
            source: &source,
            target: &target,
        },
    )?;

    let text = sandbox.text("config.toml")?;
    assert!(text.contains("# my notes"));
    assert!(text.contains("trust_level   = \"trusted\" # keep"));
    Ok(())
}

#[test]
fn codex_writes_generated_env_keys_and_records_them_in_the_sidecar() -> Result<()> {
    let sandbox = Sandbox::new("codex-env")?;
    let source = sandbox.put(
        "generated.toml",
        "[shell_environment_policy.set]\nNVIM_LOG_FILE = \"/nvim.log\"\n",
    )?;
    let target = sandbox.path("config.toml");

    sandbox.sync(
        ProviderId::Codex,
        &super::SyncPaths {
            source: &source,
            target: &target,
        },
    )?;

    assert!(
        sandbox
            .text("config.toml")?
            .contains("NVIM_LOG_FILE = \"/nvim.log\"")
    );
    assert_eq!(
        sandbox.text(".agent-harness-managed.json")?,
        "{\n  \"shell_environment_policy_set\": [\n    \"NVIM_LOG_FILE\"\n  ]\n}\n",
    );
    Ok(())
}

#[test]
fn codex_removes_a_previously_managed_env_key_and_keeps_user_added_ones() -> Result<()> {
    let sandbox = Sandbox::new("codex-env-removal")?;
    let with_key = sandbox.put(
        "with.toml",
        "[shell_environment_policy.set]\nNVIM_LOG_FILE = \"/nvim.log\"\n",
    )?;
    let without_key = sandbox.put("without.toml", "model = \"m\"\n")?;
    let target = sandbox.put(
        "config.toml",
        "[shell_environment_policy.set]\nNODE_REPL = \"abc\"\n",
    )?;

    sandbox.sync(
        ProviderId::Codex,
        &super::SyncPaths {
            source: &with_key,
            target: &target,
        },
    )?;
    sandbox.sync(
        ProviderId::Codex,
        &super::SyncPaths {
            source: &without_key,
            target: &target,
        },
    )?;

    let text = sandbox.text("config.toml")?;
    assert!(text.contains("NODE_REPL = \"abc\""));
    assert!(!text.contains("NVIM_LOG_FILE"));
    Ok(())
}

#[test]
fn codex_writes_env_keys_into_inline_tables() -> Result<()> {
    let sandbox = Sandbox::new("codex-inline")?;
    let source = sandbox.put(
        "generated.toml",
        "[shell_environment_policy.set]\nNVIM_LOG_FILE = \"/nvim.log\"\n",
    )?;
    let target = sandbox.put(
        "config.toml",
        "shell_environment_policy = { set = { CI = \"1\" } }\n",
    )?;

    sandbox.sync(
        ProviderId::Codex,
        &super::SyncPaths {
            source: &source,
            target: &target,
        },
    )?;

    let merged: toml_edit::DocumentMut = sandbox.text("config.toml")?.parse()?;
    let set = &merged["shell_environment_policy"]["set"];
    assert_eq!(set["CI"].as_str(), Some("1"));
    assert_eq!(set["NVIM_LOG_FILE"].as_str(), Some("/nvim.log"));
    Ok(())
}

#[test]
fn codex_rejects_a_shell_environment_policy_that_is_not_a_table() -> Result<()> {
    let sandbox = Sandbox::new("codex-non-table")?;
    let source = sandbox.put(
        "generated.toml",
        "[shell_environment_policy.set]\nNVIM_LOG_FILE = \"/x\"\n",
    )?;
    let target = sandbox.put("config.toml", "shell_environment_policy = \"inherit\"\n")?;

    assert!(
        sandbox
            .sync(
                ProviderId::Codex,
                &super::SyncPaths {
                    source: &source,
                    target: &target
                }
            )
            .is_err()
    );
    Ok(())
}

#[test]
fn codex_without_a_sidecar_removes_only_the_gh_config_dir_it_managed_before_sidecars() -> Result<()>
{
    let sandbox = Sandbox::new("codex-legacy")?;
    let source = sandbox.put("generated.toml", "model = \"m\"\n")?;
    let target = sandbox.put(
        "config.toml",
        "[shell_environment_policy.set]\nGH_CONFIG_DIR = \"/gh\"\nNVIM_LOG_FILE = \"/old\"\n",
    )?;

    sandbox.sync(
        ProviderId::Codex,
        &super::SyncPaths {
            source: &source,
            target: &target,
        },
    )?;

    let text = sandbox.text("config.toml")?;
    assert!(!text.contains("GH_CONFIG_DIR"));
    assert!(text.contains("NVIM_LOG_FILE = \"/old\""));
    Ok(())
}

#[test]
fn codex_keeps_a_user_owned_gh_config_dir_once_a_sidecar_exists() -> Result<()> {
    let sandbox = Sandbox::new("codex-user-gh")?;
    let source = sandbox.put("generated.toml", "model = \"m\"\n")?;
    let target = sandbox.path("config.toml");
    sandbox.sync(
        ProviderId::Codex,
        &super::SyncPaths {
            source: &source,
            target: &target,
        },
    )?;
    sandbox.put(
        "config.toml",
        "[shell_environment_policy.set]\nGH_CONFIG_DIR = \"/mine\"\n",
    )?;

    sandbox.sync(
        ProviderId::Codex,
        &super::SyncPaths {
            source: &source,
            target: &target,
        },
    )?;

    assert!(
        sandbox
            .text("config.toml")?
            .contains("GH_CONFIG_DIR = \"/mine\"")
    );
    Ok(())
}

#[test]
fn codex_survives_an_unreadable_sidecar() -> Result<()> {
    let sandbox = Sandbox::new("codex-bad-sidecar")?;
    let source = sandbox.put(
        "generated.toml",
        "[shell_environment_policy.set]\nNVIM_LOG_FILE = \"/nvim.log\"\n",
    )?;
    sandbox.put(".agent-harness-managed.json", "not json")?;

    sandbox.sync(
        ProviderId::Codex,
        &super::SyncPaths {
            source: &source,
            target: &sandbox.path("config.toml"),
        },
    )?;

    assert!(sandbox.text("config.toml")?.contains("NVIM_LOG_FILE"));
    assert!(
        sandbox
            .text(".agent-harness-managed.json")?
            .contains("NVIM_LOG_FILE")
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn codex_starts_from_an_empty_config_when_the_target_is_a_dangling_symlink() -> Result<()> {
    let sandbox = Sandbox::new("codex-dangling")?;
    let source = sandbox.put("generated.toml", "model = \"m\"\n")?;
    let target = sandbox.path("config.toml");
    crate::shell::fixtures::symlink(sandbox.path("gone.toml"), &target)?;

    sandbox.sync(
        ProviderId::Codex,
        &super::SyncPaths {
            source: &source,
            target: &target,
        },
    )?;

    assert!(sandbox.text("config.toml")?.contains("model = \"m\""));
    Ok(())
}
