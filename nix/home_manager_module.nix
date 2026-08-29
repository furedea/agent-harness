{ self }:
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.programs.agent-harness;
  sourceHasProfiles = builtins.pathExists "${cfg.source}/profiles/${cfg.profile}";
  profileSource = if sourceHasProfiles then "${cfg.source}/profiles/${cfg.profile}" else cfg.source;

  agentsMd = if cfg.agentsMd == null then "${profileSource}/AGENTS.md" else cfg.agentsMd;
  commandPermissions =
    if cfg.commandPermissions == null then
      "${profileSource}/command_permissions.json"
    else
      cfg.commandPermissions;

  claudeBaseSettings = builtins.fromJSON (
    builtins.readFile "${profileSource}/claude/settings.base.json"
  );
  claudeSettings = pkgs.writeText "agent-harness-claude-settings.json" (
    builtins.toJSON (lib.recursiveUpdate claudeBaseSettings cfg.claude.settings)
  );

  codexFormat = pkgs.formats.toml { };
  codexBaseSettings = builtins.fromTOML (builtins.readFile "${profileSource}/codex/config.toml");
  codexSettings = codexFormat.generate "agent-harness-codex-settings.toml" (
    lib.recursiveUpdate codexBaseSettings cfg.codex.settings
  );

  composedSource = pkgs.runCommand "agent-harness-${cfg.profile}-source" { } ''
    cp -R ${lib.escapeShellArg "${profileSource}/."} "$out"
    chmod -R u+w "$out"
    install -m 0644 ${lib.escapeShellArg (toString agentsMd)} "$out/AGENTS.md"
    install -m 0644 ${lib.escapeShellArg (toString commandPermissions)} \
      "$out/command_permissions.json"
    install -m 0644 ${claudeSettings} "$out/claude/settings.base.json"
    install -m 0644 ${codexSettings} "$out/codex/config.toml"
  '';

  namedSourceArgs =
    option: flag:
    lib.escapeShellArgs (
      lib.concatLists (
        lib.mapAttrsToList (name: source: [
          flag
          "${name}=${source}"
        ]) option
      )
    );
  skillArgs = namedSourceArgs cfg.skills "--extra-skill";
  hookArgs = namedSourceArgs cfg.hooks "--extra-hook";

  renderedHarness = pkgs.runCommand "agent-harness-rendered" { } ''
    ${lib.getExe cfg.package} --profile ${cfg.profile} install \
      --source ${composedSource} \
      --prefix "$out" \
      ${hookArgs}
  '';

  codexRules = pkgs.runCommand "codex-default.rules" { } ''
    ${lib.getExe cfg.package} --profile ${cfg.profile} generate-codex-rules \
      --source ${composedSource} \
      --output "$out"
  '';

  providerSkills =
    provider:
    pkgs.runCommand "${provider}-skills" { } ''
      ${lib.getExe cfg.package} --profile ${cfg.profile} generate-skills \
        --source ${composedSource} \
        --provider ${provider} \
        ${skillArgs} \
        --output "$out"
    '';
in
{
  options.programs.agent-harness = {
    enable = lib.mkEnableOption "agent harness";

    package = lib.mkOption {
      type = lib.types.package;
      inherit (self.packages.${pkgs.system}) default;
      description = "agent-harness package to use.";
    };

    source = lib.mkOption {
      type = lib.types.path;
      default = self;
      description = "Profile collection or complete flat agent-harness source tree.";
    };

    profile = lib.mkOption {
      type = lib.types.enum [ "minimal" ];
      default = "minimal";
      description = "Built-in harness profile used as the composition base.";
    };

    agentsMd = lib.mkOption {
      type = lib.types.nullOr lib.types.path;
      default = null;
      description = "Optional AGENTS.md source shared by Codex and Claude Code.";
    };

    commandPermissions = lib.mkOption {
      type = lib.types.nullOr lib.types.path;
      default = null;
      description = "Optional provider-neutral command permissions JSON source.";
    };

    skills = lib.mkOption {
      type = lib.types.attrsOf (lib.types.either lib.types.path lib.types.package);
      default = { };
      description = "Skill directories keyed by installed skill name.";
    };

    hooks = lib.mkOption {
      type = lib.types.attrsOf (lib.types.either lib.types.path lib.types.package);
      default = { };
      description = "External hook bundle directories keyed by installed bundle name.";
    };

    codex = {
      enable = lib.mkOption {
        type = lib.types.bool;
        default = true;
        description = "Whether to install Codex harness files.";
      };

      settings = lib.mkOption {
        type = lib.types.attrsOf lib.types.anything;
        default = { };
        description = "Codex settings recursively merged over the selected profile.";
      };
    };

    claude = {
      enable = lib.mkOption {
        type = lib.types.bool;
        default = true;
        description = "Whether to install Claude Code harness files.";
      };

      settings = lib.mkOption {
        type = lib.types.attrsOf lib.types.anything;
        default = { };
        description = "Claude Code settings recursively merged over the selected profile.";
      };
    };
  };

  config = lib.mkIf cfg.enable {
    home = {
      packages = [ cfg.package ];

      file = lib.mkMerge [
        (lib.mkIf cfg.codex.enable {
          ".codex/AGENTS.md".source = "${renderedHarness}/.codex/AGENTS.md";
          ".codex/hooks".source = "${renderedHarness}/.codex/hooks";
          ".codex/hooks.json".source = "${renderedHarness}/.codex/hooks.json";
          ".codex/rules/default.rules".source = codexRules;
          ".codex/skills".source = providerSkills "codex";
        })
        (lib.mkIf cfg.claude.enable {
          ".claude/CLAUDE.md".source = "${renderedHarness}/.claude/CLAUDE.md";
          ".claude/hooks".source = "${renderedHarness}/.claude/hooks";
          ".claude/skills".source = providerSkills "claude";
          ".claude/statusline".source = "${renderedHarness}/.claude/statusline";
        })
      ];

      activation.agentHarnessCodexConfig = lib.mkIf cfg.codex.enable (
        lib.hm.dag.entryAfter [ "writeBoundary" ] ''
          ${lib.getExe cfg.package} sync-codex-config \
            --source ${renderedHarness}/.codex/config.toml \
            --target "$HOME/.codex/config.toml"
        ''
      );

      activation.agentHarnessClaudeSettings = lib.mkIf cfg.claude.enable (
        lib.hm.dag.entryAfter [ "linkGeneration" ] ''
          ${lib.getExe cfg.package} sync-claude-settings \
            --source ${renderedHarness}/.claude/settings.json \
            --target "$HOME/.claude/settings.json"
        ''
      );
    };
  };
}
