use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use super::*;
use crate::config_sync::codec::ConfigTree;
use crate::config_sync::codec::Role;
use crate::layout::SourceLayout;
use crate::runtime_root::RuntimeRoot;
use crate::shell::fixtures::{create_dir_all, load_source, remove_dir_all, write};

fn test_root(name: &str) -> Result<PathBuf> {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let root = std::env::temp_dir().join(format!("agent-harness-antigravity-{name}-{nanos}"));
    create_dir_all(&root)?;
    Ok(root)
}

fn put(path: PathBuf, content: &str) -> Result<()> {
    path.parent().map(create_dir_all).transpose()?;
    write(path, content)?;
    Ok(())
}

const SECRET_POLICY: &str = r#"{"version":1,"rules":[
  {"pattern":"~/.ssh/**","access":["read"]},
  {"pattern":"~/.config/gh/hosts.yml","access":["read","write"]},
  {"pattern":"//etc/secret/","access":["read"]},
  {"pattern":"**/.env","access":["read","write"]},
  {"pattern":"*.pem","access":["read"]},
  {"pattern":"~/keys/*.pem","access":["read"]},
  {"pattern":"~/vault/**/token","access":["read"]},
  {"pattern":"~/.aws/**","access":["write"]},
  {"pattern":"~/**","access":["read"]},
  {"pattern":"//","access":["write"]}
]}"#;

const COMMANDS: &str = r#"{"version":1,"rules":[
  {"decision":"allow","prefix":["git","status"],"justification":"read only"},
  {"decision":"ask","prefix":["git","push"],"justification":"publishes"},
  {"decision":"deny","prefix":["curl"],"justification":"network"}
]}"#;

fn generated(source_settings: &Value) -> Result<Value> {
    let root = test_root("generated")?;
    put(SourceLayout::new(&root).secret_path_policy(), SECRET_POLICY)?;
    put(SourceLayout::new(&root).command_permissions(), COMMANDS)?;
    let source = load_source(&root)?;
    let base = Json::parse(&source_settings.to_string(), Role::Generated)?;
    let settings = settings_for_runtime(GenerationContext::home(&source), &base)?;
    remove_dir_all(root)?;
    Ok(settings.into_value())
}

fn rules(settings: &Value, list: &str) -> Vec<String> {
    settings["permissions"][list]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|rule| rule.as_str().map(str::to_owned))
        .collect()
}

#[test]
fn the_sandbox_may_read_nix_and_run_so_git_and_nix_start() -> Result<()> {
    let allow = rules(&generated(&json!({}))?, "allow");

    assert!(allow.contains(&"read_file(/nix)".to_owned()));
    assert!(allow.contains(&"read_file(/run)".to_owned()));
    Ok(())
}

#[test]
fn command_rules_render_as_command_prefixes_in_their_own_decision_list() -> Result<()> {
    let settings = generated(&json!({}))?;

    assert!(rules(&settings, "allow").contains(&"command(git status)".to_owned()));
    assert_eq!(rules(&settings, "ask"), ["command(git push)"]);
    assert!(rules(&settings, "deny").contains(&"command(curl)".to_owned()));
    Ok(())
}

#[test]
fn hooks_and_settings_cannot_be_written_by_the_agent() -> Result<()> {
    let deny = rules(&generated(&json!({}))?, "deny");

    for protected in [
        "write_file(~/.gemini/config)",
        "write_file(~/.gemini/config/hooks.json)",
        "write_file(~/.gemini/antigravity-cli/settings.json)",
    ] {
        assert!(
            deny.contains(&protected.to_owned()),
            "{protected}: {deny:?}"
        );
    }
    Ok(())
}

#[test]
fn secret_reads_are_denied_only_where_agy_can_name_one_location() -> Result<()> {
    let deny = rules(&generated(&json!({}))?, "deny");

    for denied in [
        "read_file(~/.ssh)",
        "read_file(~/.config/gh/hosts.yml)",
        "read_file(/etc/secret)",
    ] {
        assert!(deny.contains(&denied.to_owned()), "{denied}: {deny:?}");
    }
    assert!(
        !deny
            .iter()
            .any(|rule| rule.contains(".env") || rule.contains(".pem") || rule.contains("vault")),
        "glob secrets stay with the hook: {deny:?}"
    );
    assert!(
        !deny.contains(&"read_file(~/.aws)".to_owned()),
        "a write-only rule denies no read: {deny:?}"
    );
    assert!(
        deny.contains(&"write_file(~/.aws)".to_owned()),
        "a write rule denies the write: {deny:?}"
    );
    assert!(
        !deny.contains(&"write_file(~/.ssh)".to_owned()),
        "a read-only rule denies no write: {deny:?}"
    );
    Ok(())
}

#[test]
fn the_home_and_the_root_are_named_when_a_rule_denies_all_of_them() -> Result<()> {
    let deny = rules(&generated(&json!({}))?, "deny");

    assert!(deny.contains(&"read_file(~)".to_owned()), "{deny:?}");
    assert!(deny.contains(&"write_file(/)".to_owned()), "{deny:?}");
    Ok(())
}

#[test]
fn the_run_allow_does_not_cover_the_well_known_secret_mounts() -> Result<()> {
    let deny = rules(&generated(&json!({}))?, "deny");

    for mount in [
        "/run/secrets",
        "/run/secrets.d",
        "/run/secrets-for-users",
        "/run/agenix",
        "/run/agenix.d",
        "/run/keys",
        "/run/user",
        "/run/credentials",
    ] {
        assert!(deny.contains(&format!("read_file({mount})")), "{mount}");
    }
    Ok(())
}

#[test]
fn no_path_rule_contains_a_glob_and_none_is_edit_file() -> Result<()> {
    let settings = generated(&json!({"permissions": {"allow": ["mcp(github/get_issue)"]}}))?;

    for rule in ["allow", "ask", "deny"]
        .iter()
        .flat_map(|list| rules(&settings, list))
    {
        let is_path_rule = rule.starts_with("read_file(") || rule.starts_with("write_file(");
        assert!(
            !(is_path_rule && rule.contains(['*', '?', '['])),
            "agy rejects a glob in {rule}"
        );
        assert!(!rule.starts_with("edit_file("), "agy has no {rule}");
    }
    Ok(())
}

#[test]
fn a_source_path_rule_agy_cannot_start_with_is_rejected() {
    for rule in [
        "read_file(**/.env)",
        "write_file(relative/path)",
        "read_file(/nix/store/*)",
        "write_file(.env)",
        "read_file(/home/me/.ssh",
        "read_file(~user/.ssh)",
    ] {
        let error = generated(&json!({"permissions": {"deny": [rule]}})).unwrap_err();

        assert!(format!("{error:#}").contains(rule), "{rule}: {error:#}");
    }
}

#[test]
fn agent_mode_is_default_and_the_terminal_sandbox_is_on() -> Result<()> {
    let settings = generated(&json!({"agentMode": "auto", "enableTerminalSandbox": false}))?;

    assert_eq!(settings["agentMode"], json!(""));
    assert_eq!(settings["enableTerminalSandbox"], json!(true));
    Ok(())
}

#[test]
fn source_settings_and_rules_are_kept_and_not_repeated() -> Result<()> {
    let settings = generated(&json!({
        "modelProvider": "gemini",
        "permissions": {
            "allow": ["mcp(github/get_issue)", "read_file(/nix)"],
            "deny": ["read_file(~/.config/agent-harness/key.pem)"]
        }
    }))?;

    assert_eq!(settings["modelProvider"], json!("gemini"));
    assert!(rules(&settings, "allow").contains(&"mcp(github/get_issue)".to_owned()));
    assert_eq!(
        rules(&settings, "allow")
            .iter()
            .filter(|rule| *rule == "read_file(/nix)")
            .count(),
        1
    );
    assert!(
        rules(&settings, "deny").contains(&"read_file(~/.config/agent-harness/key.pem)".to_owned())
    );
    Ok(())
}

#[test]
fn a_directory_runtime_root_writes_absolute_protected_paths() -> Result<()> {
    let root = test_root("directory-root")?;
    put(SourceLayout::new(&root).command_permissions(), COMMANDS)?;
    let source = load_source(&root)?;
    let project = RuntimeRoot::directory(PathBuf::from("/work/project"))?;
    let context = GenerationContext {
        runtime_root: &project,
        ..GenerationContext::home(&source)
    };

    let settings = settings_for_runtime(context, &Json::parse("{}", Role::Generated)?)?;

    let deny = rules(&settings.into_value(), "deny");
    assert!(deny.contains(&"write_file(/work/project/.gemini/config)".to_owned()));
    remove_dir_all(root)?;
    Ok(())
}

#[test]
fn a_generated_path_with_a_parenthesis_or_glob_character_is_rejected() -> Result<()> {
    let root = test_root("unsafe-runtime-root")?;
    let source = load_source(&root)?;
    for directory in ["/work/a(b", "/work/a*b"] {
        let project = RuntimeRoot::directory(PathBuf::from(directory))?;
        let context = GenerationContext {
            runtime_root: &project,
            ..GenerationContext::home(&source)
        };

        let error = settings_for_runtime(context, &Json::parse("{}", Role::Generated)?);

        assert!(error.is_err(), "{directory}");
    }
    remove_dir_all(root)?;
    Ok(())
}
