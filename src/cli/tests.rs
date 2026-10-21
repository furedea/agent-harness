use clap::{CommandFactory, Parser};

use super::{Cli, Command, GithubAppTokenCommand, Profile};

#[test]
fn commands_default_to_the_minimal_profile() {
    let cli = Cli::try_parse_from(["agent-harness", "list"]).unwrap();

    assert_eq!(cli.profile, Profile::Minimal);
}

#[test]
fn every_top_level_command_has_a_description() {
    let command = Cli::command();

    for subcommand in command.get_subcommands() {
        assert!(
            subcommand.get_about().is_some(),
            "{} has no description",
            subcommand.get_name(),
        );
    }
}

#[test]
fn exposes_command_permissions_generator() {
    let command = Cli::command();

    assert!(
        command
            .find_subcommand("generate-command-permissions")
            .is_some()
    );
    assert!(command.find_subcommand("generate-command-policy").is_none());
}

#[test]
fn exposes_claude_settings_sync() {
    let command = Cli::command();

    assert!(command.find_subcommand("sync-claude-settings").is_some());
}

#[test]
fn github_app_token_command_exposes_refresh_and_status() {
    let cli = Cli::try_parse_from([
        "agent-harness",
        "github-app-token",
        "refresh",
        "--app-id",
        "12345",
        "--private-key",
        "/keys/app.pem",
        "--config-dir",
        "/home/test/.config/agent-harness/gh",
    ]);
    assert!(cli.is_ok(), "{}", cli.unwrap_err());

    let cli = Cli::try_parse_from([
        "agent-harness",
        "github-app-token",
        "status",
        "--config-dir",
        "/home/test/.config/agent-harness/gh",
    ]);
    assert!(cli.is_ok(), "{}", cli.unwrap_err());
}

#[test]
fn github_app_token_refresh_accepts_force() {
    let cli = Cli::try_parse_from([
        "agent-harness",
        "github-app-token",
        "refresh",
        "--app-id",
        "12345",
        "--private-key",
        "/keys/app.pem",
        "--config-dir",
        "/home/test/.config/agent-harness/gh",
        "--force",
    ])
    .unwrap();

    let Command::GithubAppToken(GithubAppTokenCommand::Refresh(args)) = cli.command else {
        panic!("expected github-app-token refresh");
    };
    assert!(args.force);
}

#[test]
fn list_command_exposes_inventory_subcommands() {
    let command = Cli::command();
    let list = command.find_subcommand("list").unwrap();

    for name in ["hooks", "skills"] {
        assert!(list.find_subcommand(name).is_some(), "missing {name}");
    }
}

#[test]
fn list_command_accepts_a_source_tree() {
    let command = Cli::command();
    let list = command.find_subcommand("list").unwrap();
    let source = list
        .get_arguments()
        .find(|argument| argument.get_id() == "source")
        .unwrap();

    assert_eq!(source.get_long(), Some("source"));
}

#[test]
fn verify_command_accepts_a_source_tree() {
    let command = Cli::command();
    let verify = command.find_subcommand("verify").unwrap();
    let source = verify
        .get_arguments()
        .find(|argument| argument.get_id() == "source")
        .unwrap();

    assert_eq!(source.get_long(), Some("source"));
}
