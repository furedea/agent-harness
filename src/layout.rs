use crate::provider::spec::{ProviderId, ProviderSpec};
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

const CLAUDE_AGENT_INSTRUCTIONS_PATH: &str = ".claude/CLAUDE.md";
const CLAUDE_ALLOWED_COMMAND_RULES_PATH: &str = ".claude/hooks/rules/allowed_commands.json";
const CLAUDE_COMMAND_PERMISSIONS_PATH: &str = ".claude/hooks/rules/command_permissions.json";
const CLAUDE_FORBIDDEN_COMMAND_RULES_PATH: &str = ".claude/hooks/rules/forbidden_commands.json";
const CLAUDE_PROTECTED_PATHS_PATH: &str = ".claude/hooks/rules/protected_paths.json";
const CLAUDE_SECRET_COMMIT_POLICY_PATH: &str = ".claude/hooks/rules/secret_commit_policy.json";
const CLAUDE_SECRET_PATH_POLICY_PATH: &str = ".claude/hooks/rules/secret_path_policy.json";
const CLAUDE_SETTINGS_PATH: &str = ".claude/settings.json";
const CODEX_AGENT_INSTRUCTIONS_PATH: &str = ".codex/AGENTS.md";
const CODEX_HOOK_CONFIG_PATH: &str = ".codex/hooks.json";
const CODEX_RULES_PATH: &str = ".codex/rules/default.rules";

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

    pub(crate) fn agent_hooks(self) -> PathBuf {
        self.root.join("hooks")
    }

    pub(crate) fn codex_hooks(self) -> PathBuf {
        self.root.join("codex/hooks")
    }

    fn hook_sync_paths(
        self,
        installed: InstalledLayout,
        spec: &ProviderSpec,
    ) -> (PathBuf, PathBuf) {
        (
            self.root.join(spec.source_hooks),
            installed.root.join(spec.hooks_dir),
        )
    }

    pub(crate) fn claude_statusline(self) -> PathBuf {
        self.root.join("claude/statusline")
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

    pub(crate) fn codex_agent_instructions(self) -> PathBuf {
        self.root.join(CODEX_AGENT_INSTRUCTIONS_PATH)
    }

    pub(crate) fn claude_agent_instructions(self) -> PathBuf {
        self.root.join(CLAUDE_AGENT_INSTRUCTIONS_PATH)
    }

    pub(crate) fn codex_hooks(self) -> PathBuf {
        self.root.join(".codex/hooks")
    }

    pub(crate) fn claude_hooks(self) -> PathBuf {
        self.root.join(".claude/hooks")
    }

    pub(crate) fn hermes_config(self) -> PathBuf {
        self.root
            .join(ProviderId::Hermes.spec().home_dir)
            .join("config.yaml")
    }

    pub(crate) fn claude_statusline(self) -> PathBuf {
        self.root.join(".claude/statusline")
    }

    pub(crate) fn codex_hook_config(self) -> PathBuf {
        self.root.join(CODEX_HOOK_CONFIG_PATH)
    }

    pub(crate) fn codex_skills(self) -> PathBuf {
        self.root.join(".codex/skills")
    }

    pub(crate) fn claude_skills(self) -> PathBuf {
        self.root.join(".claude/skills")
    }

    pub(crate) fn claude_settings(self) -> PathBuf {
        self.root.join(CLAUDE_SETTINGS_PATH)
    }

    pub(crate) fn codex_rules(self) -> PathBuf {
        self.root.join(CODEX_RULES_PATH)
    }

    pub(crate) fn claude_command_permissions(self) -> PathBuf {
        self.root.join(CLAUDE_COMMAND_PERMISSIONS_PATH)
    }

    pub(crate) fn claude_allowed_command_rules(self) -> PathBuf {
        self.root.join(CLAUDE_ALLOWED_COMMAND_RULES_PATH)
    }

    pub(crate) fn claude_forbidden_command_rules(self) -> PathBuf {
        self.root.join(CLAUDE_FORBIDDEN_COMMAND_RULES_PATH)
    }

    pub(crate) fn claude_protected_paths(self) -> PathBuf {
        self.root.join(CLAUDE_PROTECTED_PATHS_PATH)
    }

    pub(crate) fn claude_secret_path_policy(self) -> PathBuf {
        self.root.join(CLAUDE_SECRET_PATH_POLICY_PATH)
    }

    pub(crate) fn claude_secret_commit_policy(self) -> PathBuf {
        self.root.join(CLAUDE_SECRET_COMMIT_POLICY_PATH)
    }

    pub(crate) fn codex_config(self) -> PathBuf {
        self.root.join(".codex/config.toml")
    }

    pub(crate) fn managed_directories_for(self, providers: &[ProviderSpec]) -> Vec<PathBuf> {
        let mut paths = vec![self.codex_hooks(), self.claude_hooks()];
        paths.extend(
            providers
                .iter()
                .flat_map(provider_directories)
                .map(|path| self.root.join(path)),
        );
        paths.extend([
            self.claude_statusline(),
            self.codex_skills(),
            self.claude_skills(),
        ]);
        paths
    }

    pub(crate) fn managed_files_for(self, providers: &[ProviderSpec]) -> Vec<PathBuf> {
        let mut files = vec![
            self.codex_agent_instructions(),
            self.claude_agent_instructions(),
            self.codex_hook_config(),
            self.claude_settings(),
            self.codex_rules(),
            self.codex_config(),
        ];
        files.extend(
            providers
                .iter()
                .flat_map(provider_files)
                .map(|path| self.root.join(path)),
        );
        files.extend(
            providers
                .iter()
                .filter(|spec| spec.managed_asset_directory.is_none())
                .flat_map(|spec| spec.assets.iter())
                .map(|asset| self.root.join(asset.destination)),
        );
        files
    }

    pub(crate) fn synced_directories_for(
        self,
        source: SourceLayout,
        providers: &[ProviderSpec],
    ) -> Vec<(PathBuf, PathBuf)> {
        let mut paths = vec![
            (source.codex_hooks(), self.codex_hooks()),
            (source.agent_hooks(), self.claude_hooks()),
        ];
        paths.extend(
            providers
                .iter()
                .map(|spec| source.hook_sync_paths(self, spec)),
        );
        paths.push((source.claude_statusline(), self.claude_statusline()));
        paths
    }

    /// Generated files plus the mutable provider configs that installation syncs them into.
    pub(crate) fn generated_files_for(
        self,
        dsh_profile: &str,
        providers: &[ProviderSpec],
    ) -> Vec<PathBuf> {
        let mut files = self.managed_files_for(providers);
        files.extend([
            self.claude_command_permissions(),
            self.claude_protected_paths(),
            self.hermes_config(),
            self.root
                .join(ProviderId::Dsh.spec().home_dir)
                .join("profiles")
                .join(dsh_profile)
                .join("package.json"),
        ]);
        files
    }
}

fn provider_directories(spec: &ProviderSpec) -> impl Iterator<Item = &str> {
    std::iter::once(spec.hooks_dir)
        .chain(spec.managed_asset_directory)
        .chain(spec.skills)
}

fn provider_files(spec: &ProviderSpec) -> impl Iterator<Item = &str> {
    std::iter::once(spec.hook_manifest)
        .chain(spec.managed_config)
        .chain(spec.instructions)
}
