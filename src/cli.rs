use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};

use crate::{
    generation::{
        codex_config, command_policy, external_hooks::ExternalHookBundle, hook_bundle, hooks,
        protection, skills::ExternalSkill,
    },
    inventory::Inventory,
    render, source,
};

#[derive(Debug, Parser)]
#[command(version, about = "Install, inspect, and verify AI agent harness files")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Generate complete Claude Code settings.
    GenerateClaudeSettings(GenerateFileArgs),
    /// Generate Claude Code hook configuration.
    GenerateClaudeHooks(GenerateFileArgs),
    /// Generate complete managed Codex configuration.
    GenerateCodexConfigSource(GenerateFileArgs),
    /// Generate the protected-files Codex fragment.
    GenerateCodexConfigFragment(GenerateFileArgs),
    /// Generate Codex hook configuration.
    GenerateCodexHooks(GenerateFileArgs),
    /// Generate Codex execpolicy rules.
    GenerateCodexRules(GenerateFileArgs),
    /// Generate the shared runtime command prefix policy.
    GenerateCommandPolicy(GenerateFileArgs),
    /// Generate Claude Code forbidden-command rules.
    GenerateForbiddenCommands(GenerateFileArgs),
    /// Generate an isolated external hook bundle from installer commands.
    GenerateHookBundle(GenerateHookBundleArgs),
    /// Render provider-specific skill directories.
    GenerateSkills(GenerateSkillsArgs),
    /// Install all managed files under a target prefix.
    Install(InstallArgs),
    /// Inspect the Agent Harness inventory.
    List(ListArgs),
    /// Merge managed keys into an existing Codex config.
    SyncCodexConfig(SyncCodexConfigArgs),
    /// Verify that required managed files are installed.
    Verify(VerifyArgs),
}

#[derive(Debug, clap::Args)]
struct ListArgs {
    /// Read inventory data from the specified agent-harness source tree.
    #[arg(long, global = true)]
    source: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<ListCommand>,
}

#[derive(Debug, Subcommand)]
enum ListCommand {
    /// List hooks grouped by provider, event, and matcher.
    Hooks(ListHooksArgs),
    /// List skill titles and invocation modes.
    Skills,
}

#[derive(Debug, clap::Args)]
struct ListHooksArgs {
    /// Show hooks for one provider.
    #[arg(long, value_enum)]
    provider: Option<Provider>,
}

#[derive(Debug, clap::Args)]
struct GenerateFileArgs {
    #[arg(long)]
    source: Option<PathBuf>,

    #[arg(short, long)]
    output: PathBuf,

    #[arg(long, value_name = "NAME=PATH")]
    extra_hook: Vec<ExternalHookBundle>,
}

#[derive(Debug, clap::Args)]
struct GenerateHookBundleArgs {
    #[arg(long)]
    spec: PathBuf,

    #[arg(short, long)]
    output: PathBuf,
}

#[derive(Debug, clap::Args)]
struct GenerateSkillsArgs {
    #[arg(long)]
    source: Option<PathBuf>,

    #[arg(long, value_enum)]
    provider: Provider,

    #[arg(long, value_name = "NAME=PATH")]
    extra_skill: Vec<ExternalSkill>,

    #[arg(short, long)]
    output: PathBuf,
}

#[derive(Debug, clap::Args)]
struct InstallArgs {
    #[arg(long)]
    source: Option<PathBuf>,

    #[arg(long)]
    prefix: Option<PathBuf>,

    #[arg(long, value_name = "NAME=PATH")]
    extra_hook: Vec<ExternalHookBundle>,
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
        Command::GenerateCommandPolicy(args) => write_command_policy(args),
        Command::GenerateForbiddenCommands(args) => write_forbidden_commands(args),
        Command::GenerateHookBundle(args) => hook_bundle::generate(&args.spec, &args.output),
        Command::GenerateSkills(args) => generate_skills(args),
        Command::Install(args) => install(args),
        Command::List(args) => list(args),
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
    let source = source::resolve_source(args.source)?;
    crate::generation::claude_config::write_settings(
        source.as_path(),
        &args.output,
        &args.extra_hook,
    )
}

fn write_claude_hooks(args: GenerateFileArgs) -> Result<()> {
    let source = source::resolve_source(args.source)?;
    hooks::write_claude_hooks(source.as_path(), &args.output, &args.extra_hook)
}

fn generate_codex_config_source(args: GenerateFileArgs) -> Result<()> {
    let source = source::resolve_source(args.source)?;
    codex_config::write_config_source(source.as_path(), &args.output, &args.extra_hook)
}

fn write_codex_config_fragment(args: GenerateFileArgs) -> Result<()> {
    let source = source::resolve_source(args.source)?;
    protection::write_codex_config_fragment(source.as_path(), &args.extra_hook, &args.output)
}

fn write_codex_hooks(args: GenerateFileArgs) -> Result<()> {
    let source = source::resolve_source(args.source)?;
    hooks::write_codex_hooks(source.as_path(), &args.output, &args.extra_hook)
}

fn write_codex_rules(args: GenerateFileArgs) -> Result<()> {
    generate_file(args, command_policy::write_codex_rules)
}

fn write_command_policy(args: GenerateFileArgs) -> Result<()> {
    generate_file(args, command_policy::write_runtime_policy)
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
    render::generate_skills(
        source.as_path(),
        args.provider.into(),
        &args.extra_skill,
        &args.output,
    )
}

fn install(args: InstallArgs) -> Result<()> {
    let source = source::resolve_source(args.source)?;
    let prefix = args.prefix.unwrap_or_else(default_home_dir);
    render::install(source.as_path(), &prefix, &args.extra_hook)
}

fn list(args: ListArgs) -> Result<()> {
    let source = source::resolve_source(args.source)?;
    let inventory = Inventory::load(source.as_path())?;
    let output = match args.command {
        Some(ListCommand::Hooks(hook_args)) => {
            inventory.hooks(hook_args.provider.map(Provider::hook_provider))
        }
        Some(ListCommand::Skills) => inventory.skills(),
        None => inventory.summary(),
    };
    write_stdout(&output)
}

fn write_stdout(content: &str) -> Result<()> {
    use std::io::Write;

    std::io::stdout()
        .write_all(content.as_bytes())
        .context("failed to write command output")
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

impl Provider {
    fn hook_provider(self) -> hooks::HookProvider {
        match self {
            Self::Claude => hooks::HookProvider::Claude,
            Self::Codex => hooks::HookProvider::Codex,
        }
    }
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::Cli;

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
}
