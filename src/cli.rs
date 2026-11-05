use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};

use crate::{
    generation::{
        GenerationContext, claude_config, codex_config, command_permissions, devin_config,
        external_hooks::ExternalHookBundle,
        hermes_config, hook_bundle, hooks, protection,
        skills::{ExternalSkill, SkillTarget},
    },
    github_app::{self, GitHubRestApi, RefreshRequest},
    inventory::Inventory,
    profile::Profile,
    provider::spec::{PROVIDERS, ProviderSpec},
    render, runtime,
    runtime_root::RuntimeRoot,
    source,
};

#[derive(Debug, Parser)]
#[command(version, about = "Install, inspect, and verify AI agent harness files")]
struct Cli {
    /// Select the built-in harness profile.
    #[arg(long, global = true, value_enum, default_value_t = Profile::Minimal)]
    profile: Profile,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Generate hook configuration for a provider.
    GenerateHooks(GenerateHooksArgs),
    /// Sync generated configuration for a provider.
    SyncConfig(ProviderSyncArgs),
    /// Generate complete Claude Code settings.
    GenerateClaudeSettings(GenerateFileArgs),
    /// Generate Claude Code hook configuration.
    #[command(hide = true)]
    GenerateClaudeHooks(GenerateFileArgs),
    /// Generate complete managed Codex configuration.
    GenerateCodexConfigSource(GenerateFileArgs),
    /// Generate the protected-files Codex fragment.
    GenerateCodexConfigFragment(GenerateFileArgs),
    /// Generate Codex hook configuration.
    #[command(hide = true)]
    GenerateCodexHooks(GenerateFileArgs),
    /// Generate the Devin CLI hooks config fragment.
    #[command(hide = true)]
    GenerateDevinHooks(GenerateFileArgs),
    /// Generate the Hermes Agent hook manifest consumed by the plugin.
    #[command(hide = true)]
    GenerateHermesHooks(GenerateFileArgs),
    /// Generate the pi hook manifest consumed by the bridge extension.
    #[command(hide = true)]
    GeneratePiHooks(GenerateFileArgs),
    /// Generate the `DeepSeek` Harness hook manifest.
    #[command(hide = true)]
    GenerateDshHooks(GenerateFileArgs),
    /// Generate Codex execpolicy rules.
    GenerateCodexRules(GenerateFileArgs),
    /// Generate the shared runtime command permissions.
    GenerateCommandPermissions(GenerateFileArgs),
    /// Generate Claude Code forbidden-command rules.
    GenerateForbiddenCommands(GenerateFileArgs),
    /// Generate an isolated external hook bundle from installer commands.
    GenerateHookBundle(GenerateHookBundleArgs),
    /// Render provider-specific skill directories.
    GenerateSkills(GenerateSkillsArgs),
    /// Mint or inspect the GitHub App token used as the agent GitHub token.
    #[command(subcommand)]
    GithubAppToken(GithubAppTokenCommand),
    /// Install all managed files under a target prefix.
    Install(InstallArgs),
    /// Inspect the Agent Harness inventory.
    List(ListArgs),
    /// Merge managed keys into an existing Codex config.
    #[command(hide = true)]
    SyncCodexConfig(SyncConfigArgs),
    /// Merge generated top-level keys into existing Claude Code settings.
    #[command(hide = true)]
    SyncClaudeSettings(SyncConfigArgs),
    /// Merge generated top-level keys into an existing Devin CLI config.
    #[command(hide = true)]
    SyncDevinConfig(SyncConfigArgs),
    /// Merge managed keys into an existing Hermes Agent config.
    #[command(hide = true)]
    SyncHermesConfig(SyncConfigArgs),
    /// Register an embedded bridge bundle in a dsh profile package.json.
    #[command(hide = true)]
    SyncDshConfig(SyncConfigArgs),
    /// Verify managed files and source-declared runtime commands.
    Verify(VerifyArgs),
}

#[derive(Debug, Subcommand)]
enum GithubAppTokenCommand {
    /// Mint an installation access token into a gh config directory when the current one expires soon.
    Refresh(GithubAppTokenRefreshArgs),
    /// Report the token expiry, failing when the token is stale or the last refresh failed.
    Status(GithubAppTokenStatusArgs),
}

#[derive(Debug, clap::Args)]
struct GithubAppTokenRefreshArgs {
    #[arg(long)]
    app_id: u64,

    /// Defaults to the App's only installation.
    #[arg(long)]
    installation_id: Option<u64>,

    #[arg(long, value_name = "PATH")]
    private_key: PathBuf,

    #[arg(long, value_name = "PATH")]
    config_dir: PathBuf,

    /// Mint even when the current token has at least 20 minutes left.
    #[arg(long)]
    force: bool,
}

#[derive(Debug, clap::Args)]
struct GithubAppTokenStatusArgs {
    #[arg(long, value_name = "PATH")]
    config_dir: PathBuf,
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
    provider: Option<hooks::HookProvider>,
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
struct GenerateHooksArgs {
    #[arg(long, value_enum)]
    provider: hooks::HookProvider,
    #[command(flatten)]
    file: GenerateFileArgs,
}

#[derive(Debug, clap::Args)]
struct ProviderSyncArgs {
    #[arg(long, value_enum)]
    provider: hooks::HookProvider,
    #[command(flatten)]
    config: SyncConfigArgs,
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
    #[arg(long, default_value = "web")]
    dsh_profile: String,

    #[arg(long)]
    source: Option<PathBuf>,

    #[arg(long)]
    prefix: Option<PathBuf>,

    /// Resolve installed hook assets from this absolute directory at runtime.
    #[arg(long, value_name = "PATH")]
    runtime_root: Option<PathBuf>,

    #[arg(long, value_name = "NAME=PATH")]
    extra_hook: Vec<ExternalHookBundle>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Provider {
    Claude,
    Codex,
}

#[derive(Debug, clap::Args)]
struct SyncConfigArgs {
    #[arg(long)]
    source: PathBuf,

    #[arg(long)]
    target: PathBuf,
}

#[derive(Debug, clap::Args)]
struct VerifyArgs {
    /// Read runtime requirements from the specified source tree.
    #[arg(long)]
    source: Option<PathBuf>,

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
    let cli = Cli::parse();
    let profile = cli.profile;
    match cli.command {
        Command::GenerateHooks(args) => write_hooks(args.provider, args.file, profile),
        Command::SyncConfig(args) => sync_config(args.provider, &args.config),
        Command::GenerateClaudeSettings(args) => generate_claude_settings(args, profile),
        Command::GenerateClaudeHooks(args) => {
            write_hooks(hooks::HookProvider::Claude, args, profile)
        }
        Command::GenerateCodexConfigSource(args) => generate_codex_config_source(args, profile),
        Command::GenerateCodexConfigFragment(args) => write_codex_config_fragment(args, profile),
        Command::GenerateCodexHooks(args) => write_hooks(hooks::HookProvider::Codex, args, profile),
        Command::GenerateDevinHooks(args) => write_hooks(hooks::HookProvider::Devin, args, profile),
        Command::GenerateHermesHooks(args) => {
            write_hooks(hooks::HookProvider::Hermes, args, profile)
        }
        Command::GeneratePiHooks(args) => write_hooks(hooks::HookProvider::Pi, args, profile),
        Command::GenerateDshHooks(args) => write_hooks(hooks::HookProvider::Dsh, args, profile),
        Command::GenerateCodexRules(args) => write_codex_rules(args, profile),
        Command::GenerateCommandPermissions(args) => write_command_permissions(args, profile),
        Command::GenerateForbiddenCommands(args) => write_forbidden_commands(args, profile),
        Command::GenerateHookBundle(args) => hook_bundle::generate(&args.spec, &args.output),
        Command::GenerateSkills(args) => generate_skills(args, profile),
        Command::GithubAppToken(command) => github_app_token(command),
        Command::Install(args) => install(args, profile),
        Command::List(args) => list(args, profile),
        Command::SyncCodexConfig(args) => sync_config(hooks::HookProvider::Codex, &args),
        Command::SyncClaudeSettings(args) => sync_config(hooks::HookProvider::Claude, &args),
        Command::SyncDevinConfig(args) => sync_config(hooks::HookProvider::Devin, &args),
        Command::SyncHermesConfig(args) => sync_config(hooks::HookProvider::Hermes, &args),
        Command::SyncDshConfig(args) => sync_config(hooks::HookProvider::Dsh, &args),
        Command::Verify(args) => {
            let source = source::resolve_source(args.source, profile)?;
            let prefix = args.prefix.unwrap_or_else(default_home_dir);
            render::verify(&prefix)?;
            runtime::verify(source.as_path())
        }
    }
}

fn generate_claude_settings(args: GenerateFileArgs, profile: Profile) -> Result<()> {
    let source = source::resolve_source(args.source, profile)?;
    crate::generation::claude_config::write_settings(
        source.as_path(),
        &args.output,
        &args.extra_hook,
    )
}

fn generate_codex_config_source(args: GenerateFileArgs, profile: Profile) -> Result<()> {
    let source = source::resolve_source(args.source, profile)?;
    codex_config::write_config_source(source.as_path(), &args.output, &args.extra_hook)
}

fn write_codex_config_fragment(args: GenerateFileArgs, profile: Profile) -> Result<()> {
    let source = source::resolve_source(args.source, profile)?;
    protection::write_codex_config_fragment(source.as_path(), &args.extra_hook, &args.output)
}

fn registry_spec(provider: hooks::HookProvider) -> &'static ProviderSpec {
    PROVIDERS
        .iter()
        .find(|spec| hooks::hook_provider(spec.id) == provider)
        .expect("registry provider")
}

fn write_hooks(
    provider: hooks::HookProvider,
    args: GenerateFileArgs,
    profile: Profile,
) -> Result<()> {
    let source = source::resolve_source(args.source, profile)?;
    match provider {
        hooks::HookProvider::Claude => {
            hooks::write_claude_hooks(source.as_path(), &args.output, &args.extra_hook)
        }
        hooks::HookProvider::Codex => {
            hooks::write_codex_hooks(source.as_path(), &args.output, &args.extra_hook)
        }
        provider => hooks::write_provider_hooks_for_runtime(
            GenerationContext::home(source.as_path(), &args.extra_hook),
            &args.output,
            registry_spec(provider),
        ),
    }
}

fn sync_config(provider: hooks::HookProvider, args: &SyncConfigArgs) -> Result<()> {
    match provider {
        hooks::HookProvider::Claude => {
            claude_config::sync_settings(&args.source, &args.target, &default_home_dir())
        }
        hooks::HookProvider::Codex => codex_config::sync_managed_config(&args.source, &args.target),
        hooks::HookProvider::Devin => devin_config::sync_config(&args.source, &args.target),
        hooks::HookProvider::Hermes => hermes_config::sync_config(&args.source, &args.target),
        hooks::HookProvider::Dsh => {
            crate::generation::dsh_config::sync_config(&args.source, &args.target)
        }
        hooks::HookProvider::Pi => anyhow::bail!("pi has no managed config to sync"),
    }
}

fn write_codex_rules(args: GenerateFileArgs, profile: Profile) -> Result<()> {
    generate_file(args, profile, command_permissions::write_codex_rules)
}

fn write_command_permissions(args: GenerateFileArgs, profile: Profile) -> Result<()> {
    generate_file(args, profile, command_permissions::write_runtime_policy)
}

fn write_forbidden_commands(args: GenerateFileArgs, profile: Profile) -> Result<()> {
    generate_file(args, profile, command_permissions::write_forbidden_commands)
}

fn generate_file(
    args: GenerateFileArgs,
    profile: Profile,
    generate: impl FnOnce(&Path, &Path) -> Result<()>,
) -> Result<()> {
    let source = source::resolve_source(args.source, profile)?;
    generate(source.as_path(), &args.output)
}

// Skills and install stay explicit because they add provider and prefix handling
// beyond the shared source/output file generation path.
fn generate_skills(args: GenerateSkillsArgs, profile: Profile) -> Result<()> {
    let source = source::resolve_source(args.source, profile)?;
    render::generate_skills(
        source.as_path(),
        &args.extra_skill,
        SkillTarget {
            provider: args.provider.into(),
            path: &args.output,
        },
    )
}

fn install(args: InstallArgs, profile: Profile) -> Result<()> {
    let source = source::resolve_source(args.source, profile)?;
    let prefix = args.prefix.unwrap_or_else(default_home_dir);
    let runtime_root = args
        .runtime_root
        .map(RuntimeRoot::directory)
        .transpose()?
        .unwrap_or_else(RuntimeRoot::home);
    let context = GenerationContext {
        dsh_profile: &args.dsh_profile,
        source: source.as_path(),
        external_hooks: &args.extra_hook,
        runtime_root: &runtime_root,
    };
    render::install(context, &prefix)
}

fn github_app_token(command: GithubAppTokenCommand) -> Result<()> {
    let now_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before the Unix epoch")?
        .as_secs();
    match command {
        GithubAppTokenCommand::Refresh(args) => {
            let request = RefreshRequest {
                app_id: args.app_id,
                installation_id: args.installation_id,
                private_key: args.private_key,
                config_dir: args.config_dir,
                force: args.force,
            };
            github_app::refresh(&request, &GitHubRestApi::new(), now_unix)
        }
        GithubAppTokenCommand::Status(args) => {
            write_stdout(&github_app::status(&args.config_dir, now_unix)?)
        }
    }
}

fn list(args: ListArgs, profile: Profile) -> Result<()> {
    let source = source::resolve_source(args.source, profile)?;
    let inventory = Inventory::load(source.as_path())?;
    let output = match args.command {
        Some(ListCommand::Hooks(hook_args)) => inventory.hooks(hook_args.provider),
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

#[cfg(test)]
mod tests;
