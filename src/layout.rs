use crate::provider::spec::ProviderId;
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

    pub(crate) fn devin_hooks(self) -> PathBuf {
        self.root.join(ProviderId::Devin.spec().source_hooks)
    }

    pub(crate) fn hermes_hooks(self) -> PathBuf {
        self.root.join(ProviderId::Hermes.spec().source_hooks)
    }

    pub(crate) fn pi_hooks(self) -> PathBuf {
        self.root.join(ProviderId::Pi.spec().source_hooks)
    }

    pub(crate) fn dsh_hooks(self) -> PathBuf {
        self.root.join(ProviderId::Dsh.spec().source_hooks)
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

    pub(crate) fn devin_hooks(self) -> PathBuf {
        self.root.join(ProviderId::Devin.spec().hooks_dir)
    }

    pub(crate) fn devin_config(self) -> PathBuf {
        self.root.join(ProviderId::Devin.spec().hook_manifest)
    }

    pub(crate) fn hermes_hooks(self) -> PathBuf {
        self.root.join(ProviderId::Hermes.spec().hooks_dir)
    }

    pub(crate) fn hermes_plugin(self) -> PathBuf {
        self.root
            .join(ProviderId::Hermes.spec().bridge_path.unwrap())
    }

    pub(crate) fn hermes_manifest(self) -> PathBuf {
        self.root.join(ProviderId::Hermes.spec().hook_manifest)
    }

    pub(crate) fn hermes_managed_config(self) -> PathBuf {
        self.root
            .join(ProviderId::Hermes.spec().home_dir)
            .join("managed-config.yaml")
    }

    pub(crate) fn hermes_config(self) -> PathBuf {
        self.root
            .join(ProviderId::Hermes.spec().home_dir)
            .join("config.yaml")
    }

    pub(crate) fn pi_hooks(self) -> PathBuf {
        self.root.join(ProviderId::Pi.spec().hooks_dir)
    }

    pub(crate) fn pi_hook_bridge(self) -> PathBuf {
        self.root.join(ProviderId::Pi.spec().bridge_path.unwrap())
    }

    pub(crate) fn pi_manifest(self) -> PathBuf {
        self.root.join(ProviderId::Pi.spec().hook_manifest)
    }

    pub(crate) fn dsh_hooks(self) -> PathBuf {
        self.root.join(ProviderId::Dsh.spec().hooks_dir)
    }

    pub(crate) fn dsh_bridge(self) -> PathBuf {
        self.root.join(ProviderId::Dsh.spec().bridge_path.unwrap())
    }

    pub(crate) fn dsh_manifest(self) -> PathBuf {
        self.root.join(ProviderId::Dsh.spec().hook_manifest)
    }

    pub(crate) fn dsh_skills(self) -> PathBuf {
        self.root
            .join(ProviderId::Dsh.spec().home_dir)
            .join("skills")
    }
    pub(crate) fn dsh_instructions(self) -> PathBuf {
        self.root
            .join(ProviderId::Dsh.spec().home_dir)
            .join("AGENTS.md")
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

    pub(crate) fn managed_directories(self) -> [PathBuf; 12] {
        [
            self.codex_hooks(),
            self.claude_hooks(),
            self.devin_hooks(),
            self.hermes_hooks(),
            self.hermes_plugin(),
            self.pi_hooks(),
            self.dsh_hooks(),
            self.dsh_bridge(),
            self.dsh_skills(),
            self.claude_statusline(),
            self.codex_skills(),
            self.claude_skills(),
        ]
    }

    pub(crate) fn managed_files(self) -> [PathBuf; 13] {
        [
            self.codex_agent_instructions(),
            self.claude_agent_instructions(),
            self.codex_hook_config(),
            self.claude_settings(),
            self.codex_rules(),
            self.codex_config(),
            self.devin_config(),
            self.hermes_manifest(),
            self.hermes_managed_config(),
            self.pi_manifest(),
            self.dsh_manifest(),
            self.dsh_instructions(),
            self.pi_hook_bridge(),
        ]
    }

    pub(crate) fn synced_directories(self, source: SourceLayout) -> [(PathBuf, PathBuf); 7] {
        [
            (source.codex_hooks(), self.codex_hooks()),
            (source.agent_hooks(), self.claude_hooks()),
            (source.devin_hooks(), self.devin_hooks()),
            (source.hermes_hooks(), self.hermes_hooks()),
            (source.pi_hooks(), self.pi_hooks()),
            (source.dsh_hooks(), self.dsh_hooks()),
            (source.claude_statusline(), self.claude_statusline()),
        ]
    }

    /// Generated files plus the mutable provider configs that installation syncs them into.
    pub(crate) fn generated_files(self, dsh_profile: &str) -> Vec<PathBuf> {
        let mut files = self.managed_files().to_vec();
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
