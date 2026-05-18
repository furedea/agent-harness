use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};

use crate::{codex_config, render};

#[derive(Debug, Parser)]
#[command(version, about = "Install and verify AI agent harness files")]
pub struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    GenerateClaudeSettings(GenerateFileArgs),
    GenerateCodexConfigSource(GenerateFileArgs),
    Render(RenderArgs),
    Install(InstallArgs),
    SyncCodexConfig(SyncCodexConfigArgs),
    Verify(VerifyArgs),
}

#[derive(Debug, clap::Args)]
struct RenderArgs {
    #[arg(long, default_value = ".")]
    source: PathBuf,

    #[arg(long)]
    out: PathBuf,
}

#[derive(Debug, clap::Args)]
struct GenerateFileArgs {
    #[arg(long, default_value = ".")]
    source: PathBuf,

    #[arg(long)]
    out: PathBuf,
}

#[derive(Debug, clap::Args)]
struct InstallArgs {
    #[arg(long, default_value = ".")]
    source: PathBuf,

    #[arg(long)]
    home: Option<PathBuf>,

    #[arg(long, value_enum, default_value_t = InstallMode::Copy)]
    mode: InstallMode,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum InstallMode {
    Copy,
    Symlink,
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
    home: Option<PathBuf>,
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
            render::generate_claude_settings(&args.source, &args.out)
        }
        Command::GenerateCodexConfigSource(args) => {
            render::generate_codex_config_source(&args.source, &args.out)
        }
        Command::Render(args) => render::render(&args.source, &args.out),
        Command::Install(args) => {
            let home = args.home.unwrap_or_else(default_home_dir);
            render::install(&args.source, &home, args.mode.into())
        }
        Command::SyncCodexConfig(args) => {
            codex_config::sync_managed_config(&args.source, &args.target)
        }
        Command::Verify(args) => {
            let home = args.home.unwrap_or_else(default_home_dir);
            render::verify(&home)
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
