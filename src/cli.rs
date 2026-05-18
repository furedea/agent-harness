use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};

use crate::{codex_config, command_policy, hooks, render};

#[derive(Debug, Parser)]
#[command(version, about = "Install and verify AI agent harness files")]
pub struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    GenerateClaudeSettings(GenerateFileArgs),
    GenerateClaudeHooks(GenerateFileArgs),
    GenerateCodexConfigSource(GenerateFileArgs),
    GenerateCodexHooks(GenerateFileArgs),
    GenerateCodexRules(GenerateFileArgs),
    GenerateForbiddenCommands(GenerateFileArgs),
    GenerateSkills(GenerateSkillsArgs),
    Install(InstallArgs),
    SyncCodexConfig(SyncCodexConfigArgs),
    Verify(VerifyArgs),
}

#[derive(Debug, clap::Args)]
struct GenerateFileArgs {
    #[arg(long, default_value = ".")]
    source: PathBuf,

    #[arg(short, long)]
    output: PathBuf,
}

#[derive(Debug, clap::Args)]
struct GenerateSkillsArgs {
    #[arg(long, default_value = ".")]
    source: PathBuf,

    #[arg(long, value_enum)]
    provider: Provider,

    #[arg(short, long)]
    output: PathBuf,
}

#[derive(Debug, clap::Args)]
struct InstallArgs {
    #[arg(long, default_value = ".")]
    source: PathBuf,

    #[arg(long)]
    prefix: Option<PathBuf>,

    #[arg(long, value_enum, default_value_t = InstallMode::Copy)]
    mode: InstallMode,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum InstallMode {
    Copy,
    Symlink,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Provider {
    Claude,
    Codex,
}

#[derive(Debug, clap::Args)]
struct SyncCodexConfigArgs {
    #[arg(long)]
    source: PathBuf,

    #[arg(long)]
    target: PathBuf,
}

#[derive(Debug, clap::Args)]
struct VerifyArgs {
    #[arg(long)]
    prefix: Option<PathBuf>,
}

/// Parse CLI arguments and execute the selected command.
///
/// # Errors
///
/// Returns an error when the selected command fails to read, write, render, or
/// verify harness files.
pub fn run() -> Result<()> {
    match Cli::parse().command {
        Command::GenerateClaudeSettings(args) => {
            render::generate_claude_settings(&args.source, &args.output)
        }
        Command::GenerateClaudeHooks(args) => hooks::write_claude_hooks(&args.source, &args.output),
        Command::GenerateCodexConfigSource(args) => {
            render::generate_codex_config_source(&args.source, &args.output)
        }
        Command::GenerateCodexHooks(args) => hooks::write_codex_hooks(&args.source, &args.output),
        Command::GenerateCodexRules(args) => {
            command_policy::write_codex_rules(&args.source, &args.output)
        }
        Command::GenerateForbiddenCommands(args) => {
            command_policy::write_forbidden_commands(&args.source, &args.output)
        }
        Command::GenerateSkills(args) => {
            render::generate_skills(&args.source, args.provider.into(), &args.output)
        }
        Command::Install(args) => {
            let prefix = args.prefix.unwrap_or_else(default_home_dir);
            render::install(&args.source, &prefix, args.mode.into())
        }
        Command::SyncCodexConfig(args) => {
            codex_config::sync_managed_config(&args.source, &args.target)
        }
        Command::Verify(args) => {
            let prefix = args.prefix.unwrap_or_else(default_home_dir);
            render::verify(&prefix)
        }
    }
}

fn default_home_dir() -> PathBuf {
    std::env::var_os("HOME").map_or_else(|| PathBuf::from("."), PathBuf::from)
}

impl From<InstallMode> for render::InstallMode {
    fn from(mode: InstallMode) -> Self {
        match mode {
            InstallMode::Copy => Self::Copy,
            InstallMode::Symlink => Self::Symlink,
        }
    }
}

impl From<Provider> for render::Provider {
    fn from(provider: Provider) -> Self {
        match provider {
            Provider::Claude => Self::Claude,
            Provider::Codex => Self::Codex,
        }
    }
}
