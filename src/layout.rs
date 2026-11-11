use crate::provider::spec::{CommandRuleSpec, ProviderId, ProviderSpec, SkillSource};
use std::path::{Path, PathBuf};

const MANIFEST_PATH: &str = "manifest.json";
const AGENT_INSTRUCTIONS_PATH: &str = "AGENTS.md";
const COMMAND_PERMISSIONS_PATH: &str = "command_permissions.json";
const HOOK_CONFIG_PATH: &str = "hooks.json";
const ALLOWED_COMMAND_RULES_PATH: &str = "hooks/rules/allowed_commands.json";
const FORBIDDEN_COMMAND_RULES_PATH: &str = "hooks/rules/forbidden_commands.json";
const SECRET_COMMIT_POLICY_PATH: &str = "hooks/rules/secret_commit_policy.json";
const SECRET_PATH_POLICY_PATH: &str = "hooks/rules/secret_path_policy.json";
const SKILL_RENDERING_PATH: &str = "skill_rendering.json";
const CLAUDE_SETTINGS_SOURCE_PATH: &str = "claude/settings.json";
const CODEX_CONFIG_SOURCE_PATH: &str = "codex/config.toml";
const REQUIRED_SOURCE_FILES: &[&str] = &[
    MANIFEST_PATH,
    AGENT_INSTRUCTIONS_PATH,
    COMMAND_PERMISSIONS_PATH,
    HOOK_CONFIG_PATH,
    ALLOWED_COMMAND_RULES_PATH,
    FORBIDDEN_COMMAND_RULES_PATH,
    SECRET_COMMIT_POLICY_PATH,
    SECRET_PATH_POLICY_PATH,
    SKILL_RENDERING_PATH,
    CLAUDE_SETTINGS_SOURCE_PATH,
    CODEX_CONFIG_SOURCE_PATH,
];

#[derive(Clone, Copy, Debug)]
pub(crate) struct SourceLayout<'a> {
    root: &'a Path,
}

impl<'a> SourceLayout<'a> {
    pub(crate) fn new(root: &'a Path) -> Self {
        Self { root }
    }

    pub(crate) fn is_complete(self) -> bool {
        self.missing_required_files().is_empty()
    }

    pub(crate) fn missing_required_files(self) -> Vec<&'static str> {
        REQUIRED_SOURCE_FILES
            .iter()
            .copied()
            .filter(|relative| !self.root.join(relative).is_file())
            .collect()
    }

    /// Source directories the installation cannot do without: those holding the hook rule files.
    pub(crate) fn required_directories_for(self, providers: &[ProviderSpec]) -> Vec<PathBuf> {
        providers
            .iter()
            .filter(|spec| matches!(spec.command_rule, Some(CommandRuleSpec::RuleFiles { .. })))
            .map(|spec| self.root.join(spec.hook.source))
            .collect()
    }

    pub(crate) fn missing_required_directories(self, providers: &[ProviderSpec]) -> Vec<PathBuf> {
        self.required_directories_for(providers)
            .into_iter()
            .filter(|path| !path.is_dir())
            .collect()
    }

    pub(crate) fn agent_instructions(self) -> PathBuf {
        self.root.join(AGENT_INSTRUCTIONS_PATH)
    }

    pub(crate) fn command_permissions(self) -> PathBuf {
        self.root.join(COMMAND_PERMISSIONS_PATH)
    }

    pub(crate) fn hook_config(self) -> PathBuf {
        self.root.join(HOOK_CONFIG_PATH)
    }

    pub(crate) fn allowed_command_rules(self) -> PathBuf {
        self.root.join(ALLOWED_COMMAND_RULES_PATH)
    }

    pub(crate) fn forbidden_command_rules(self) -> PathBuf {
        self.root.join(FORBIDDEN_COMMAND_RULES_PATH)
    }

    pub(crate) fn secret_path_policy(self) -> PathBuf {
        self.root.join(SECRET_PATH_POLICY_PATH)
    }

    pub(crate) fn skill_rendering(self) -> PathBuf {
        self.root.join(SKILL_RENDERING_PATH)
    }

    pub(crate) fn claude_settings(self) -> PathBuf {
        self.root.join(CLAUDE_SETTINGS_SOURCE_PATH)
    }

    pub(crate) fn codex_config(self) -> PathBuf {
        self.root.join(CODEX_CONFIG_SOURCE_PATH)
    }

    pub(crate) fn skills(self) -> PathBuf {
        self.root.join("skills")
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct InstalledLayout<'a> {
    root: &'a Path,
}

impl<'a> InstalledLayout<'a> {
    pub(crate) fn new(root: &'a Path) -> Self {
        Self { root }
    }

    pub(crate) fn relative() -> Self {
        Self::new(Path::new(""))
    }

    pub(crate) fn hermes_config(self) -> PathBuf {
        self.root
            .join(ProviderId::Hermes.spec().home_dir)
            .join("config.yaml")
    }

    /// Directories published as a whole, in registry order.
    pub(crate) fn managed_directories_for(self, providers: &[ProviderSpec]) -> Vec<PathBuf> {
        providers
            .iter()
            .flat_map(managed_directories)
            .map(|path| self.root.join(path))
            .collect()
    }

    /// Files published individually, in registry order.
    pub(crate) fn managed_files_for(self, providers: &[ProviderSpec]) -> Vec<PathBuf> {
        providers
            .iter()
            .flat_map(managed_files)
            .map(|path| self.root.join(path))
            .collect()
    }

    /// Source and installed directory pairs that installation replaces from the source tree.
    pub(crate) fn synced_directories_for(
        self,
        source: SourceLayout,
        providers: &[ProviderSpec],
    ) -> Vec<(PathBuf, PathBuf)> {
        let hooks = providers
            .iter()
            .map(|spec| (spec.hook.source, spec.hook.scripts_dir));
        let directories = providers
            .iter()
            .flat_map(ProviderSpec::config_directories)
            .map(|directory| (directory.source, directory.destination));
        hooks
            .chain(directories)
            .map(|(from, to)| (source.root.join(from), self.root.join(to)))
            .collect()
    }

    /// Generated files plus the mutable provider configs that installation syncs them into.
    pub(crate) fn generated_files_for(
        self,
        dsh_profile: &str,
        providers: &[ProviderSpec],
    ) -> Vec<PathBuf> {
        let mut files = self.managed_files_for(providers);
        files.extend(
            providers
                .iter()
                .flat_map(ProviderSpec::generated_rule_files)
                .map(|path| self.root.join(path)),
        );
        files.extend([
            self.hermes_config(),
            self.root
                .join(ProviderId::Dsh.spec().home_dir)
                .join("profiles")
                .join(dsh_profile)
                .join("package.json"),
        ]);
        files
    }

    /// Files that verification requires to exist.
    pub(crate) fn verified_files_for(self, providers: &[ProviderSpec]) -> Vec<PathBuf> {
        providers
            .iter()
            .flat_map(verified_files)
            .map(|path| self.root.join(path))
            .collect()
    }

    /// Skill directories that verification requires, which rendering produces itself.
    pub(crate) fn verified_directories_for(self, providers: &[ProviderSpec]) -> Vec<PathBuf> {
        providers
            .iter()
            .filter_map(|spec| spec.skill)
            .filter(|skill| !matches!(skill.source, SkillSource::CopyRendered(_)))
            .map(|skill| self.root.join(skill.destination))
            .collect()
    }
}

fn managed_directories(spec: &ProviderSpec) -> impl Iterator<Item = &'static str> {
    let config_directories = spec
        .config_directories()
        .iter()
        .map(|directory| directory.destination);
    std::iter::once(spec.hook.scripts_dir)
        .chain(spec.hook.bridge.directory)
        .chain(config_directories)
        .chain(spec.skill.map(|skill| skill.destination))
}

/// Files published individually: rendered files first, then bridge assets outside a directory.
fn managed_files(spec: &ProviderSpec) -> impl Iterator<Item = &'static str> {
    let loose_assets = spec
        .hook
        .bridge
        .directory
        .is_none()
        .then_some(spec.hook.bridge.assets)
        .into_iter()
        .flatten()
        .map(|asset| asset.destination);
    rendered_files(spec).chain(loose_assets)
}

fn rendered_files(spec: &ProviderSpec) -> impl Iterator<Item = &'static str> {
    spec.instructions
        .into_iter()
        .chain([spec.hook.registration.path()])
        .chain(spec.config_file())
        .chain(spec.published_rule_file())
}

fn verified_files(spec: &ProviderSpec) -> impl Iterator<Item = &'static str> {
    let assets = spec.hook.bridge.assets.iter();
    rendered_files(spec)
        .chain(spec.generated_rule_files())
        .chain(spec.source_rule_files().iter().copied())
        .chain(assets.map(|asset| asset.destination))
}
