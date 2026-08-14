use std::process::Command;

#[test]
fn top_level_help_describes_every_command() {
    let output = Command::new(env!("CARGO_BIN_EXE_agent-harness"))
        .arg("-h")
        .output()
        .unwrap();
    assert!(output.status.success());

    let help = String::from_utf8(output.stdout).unwrap();
    let expected_commands = [
        (
            "generate-claude-settings",
            "Generate complete Claude Code settings",
        ),
        (
            "generate-claude-hooks",
            "Generate Claude Code hook configuration",
        ),
        (
            "generate-codex-config-source",
            "Generate complete managed Codex configuration",
        ),
        (
            "generate-codex-config-fragment",
            "Generate the protected-files Codex fragment",
        ),
        ("generate-codex-hooks", "Generate Codex hook configuration"),
        ("generate-codex-rules", "Generate Codex execpolicy rules"),
        (
            "generate-forbidden-commands",
            "Generate Claude Code forbidden-command rules",
        ),
        (
            "generate-herdr-integration",
            "Generate Claude and Codex integration files with Herdr",
        ),
        (
            "generate-skills",
            "Render provider-specific skill directories",
        ),
        ("install", "Install all managed files under a target prefix"),
        ("list", "Inspect the Agent Harness inventory"),
        (
            "sync-codex-config",
            "Merge managed keys into an existing Codex config",
        ),
        ("verify", "Verify that required managed files are installed"),
        (
            "help",
            "Print this message or the help of the given subcommand(s)",
        ),
    ];

    for (command, description) in expected_commands {
        assert!(
            help.lines().any(|line| {
                let line = line.trim_start();
                line.starts_with(command) && line.contains(description)
            }),
            "missing description for {command}: {description}\n\n{help}",
        );
    }
}
