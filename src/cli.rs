use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};

use crate::{
    generation::{codex_config, command_policy, hooks, protection},
    render, source,
};

#[derive(Debug, Parser)]
#[command(version, about = "Install and verify AI agent harness files")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    GenerateClaudeSettings(GenerateFileArgs),
    GenerateClaudeHooks(GenerateFileArgs),
    GenerateCodexConfigSource(GenerateFileArgs),
    GenerateCodexConfigFragment(GenerateFileArgs),
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
    #[arg(long)]
    source: Option<PathBuf>,

    #[arg(short, long)]
    output: PathBuf,
}

#[derive(Debug, clap::Args)]
struct GenerateSkillsArgs {
    #[arg(long)]
    source: Option<PathBuf>,

    #[arg(long, value_enum)]
    provider: Provider,

    #[arg(short, long)]
    output: PathBuf,
}

#[derive(Debug, clap::Args)]
struct InstallArgs {
    #[arg(long)]
    source: Option<PathBuf>,

    #[arg(long)]
    prefix: Option<PathBuf>,
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
        Command::GenerateClaudeSettings(args) => generate_claude_settings(args),
        Command::GenerateClaudeHooks(args) => write_claude_hooks(args),
        Command::GenerateCodexConfigSource(args) => generate_codex_config_source(args),
        Command::GenerateCodexConfigFragment(args) => write_codex_config_fragment(args),
        Command::GenerateCodexHooks(args) => write_codex_hooks(args),
        Command::GenerateCodexRules(args) => write_codex_rules(args),
        Command::GenerateForbiddenCommands(args) => write_forbidden_commands(args),
        Command::GenerateSkills(args) => generate_skills(args),
        Command::Install(args) => install(args),
        Command::SyncCodexConfig(args) => {
            codex_config::sync_managed_config(&args.source, &args.target)
        }
        Command::Verify(args) => {
            let prefix = args.prefix.unwrap_or_else(default_home_dir);
            render::verify(&prefix)
        }
    }
}

fn generate_claude_settings(args: GenerateFileArgs) -> Result<()> {
    generate_file(args, render::generate_claude_settings)
}

fn write_claude_hooks(args: GenerateFileArgs) -> Result<()> {
    generate_file(args, hooks::write_claude_hooks)
}

fn generate_codex_config_source(args: GenerateFileArgs) -> Result<()> {
    generate_file(args, render::generate_codex_config_source)
}

fn write_codex_config_fragment(args: GenerateFileArgs) -> Result<()> {
    generate_file(args, protection::write_codex_config_fragment)
}

fn write_codex_hooks(args: GenerateFileArgs) -> Result<()> {
    generate_file(args, hooks::write_codex_hooks)
}

fn write_codex_rules(args: GenerateFileArgs) -> Result<()> {
    generate_file(args, command_policy::write_codex_rules)
}

fn write_forbidden_commands(args: GenerateFileArgs) -> Result<()> {
    generate_file(args, command_policy::write_forbidden_commands)
}

fn generate_file(
    args: GenerateFileArgs,
    generate: impl FnOnce(&Path, &Path) -> Result<()>,
) -> Result<()> {
    let source = source::resolve_source(args.source)?;
    generate(source.as_path(), &args.output)
}

// Skills and install stay explicit because they add provider and prefix handling
// beyond the shared source/output file generation path.
fn generate_skills(args: GenerateSkillsArgs) -> Result<()> {
    let source = source::resolve_source(args.source)?;
    render::generate_skills(source.as_path(), args.provider.into(), &args.output)
}

fn install(args: InstallArgs) -> Result<()> {
    let source = source::resolve_source(args.source)?;
    let prefix = args.prefix.unwrap_or_else(default_home_dir);
    render::install(source.as_path(), &prefix)
}

fn default_home_dir() -> PathBuf {
    std::env::var_os("HOME").map_or_else(|| PathBuf::from("."), PathBuf::from)
}

impl From<Provider> for render::Provider {
    fn from(provider: Provider) -> Self {
        match provider {
            Provider::Claude => Self::Claude,
            Provider::Codex => Self::Codex,
        }
    }
}
