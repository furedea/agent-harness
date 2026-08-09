{ self }:
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.programs.agent-harness;
  herdrArgs = lib.optionalString cfg.herdr.enable "--herdr-integration ${herdrIntegration}";
  extraSkillArgs = lib.escapeShellArgs (
    lib.concatLists (
      lib.mapAttrsToList (name: source: [
        "--extra-skill"
        "${name}=${source}"
      ]) cfg.skills.extra
    )
  );

  herdrIntegration = pkgs.runCommand "agent-harness-herdr-integration" { } ''
    ${lib.getExe cfg.package} generate-herdr-integration \
      --herdr-bin ${lib.getExe cfg.herdr.package} \
      --output $out
  '';

  claudeSettings = pkgs.runCommand "claude-settings.json" { } ''
    ${lib.getExe cfg.package} generate-claude-settings \
      --source ${cfg.source} \
      ${herdrArgs} \
      --output $out
  '';

  codexConfigSource = pkgs.runCommand "codex-config-source.toml" { } ''
    ${lib.getExe cfg.package} generate-codex-config-source \
      --source ${cfg.source} \
      ${herdrArgs} \
      --output $out
  '';

  codexRules = pkgs.runCommand "codex-default.rules" { } ''
    ${lib.getExe cfg.package} generate-codex-rules \
      --source ${cfg.source} \
      --output $out
  '';

  codexHooks = pkgs.runCommand "codex-hooks.json" { } ''
    ${lib.getExe cfg.package} generate-codex-hooks \
      --source ${cfg.source} \
      ${herdrArgs} \
      --output $out
  '';

  claudeForbiddenCommands = pkgs.runCommand "claude-forbidden-commands.json" { } ''
    ${lib.getExe cfg.package} generate-forbidden-commands \
      --source ${cfg.source} \
      --output $out
  '';

  claudeHooks = pkgs.runCommand "claude-hooks" { } ''
    mkdir -p $out
    cp -R ${cfg.source}/agents/hooks/. $out/
    chmod -R u+w $out
    mkdir -p $out/rules
    cp ${claudeForbiddenCommands} $out/rules/forbidden_commands.json
    ${lib.optionalString cfg.herdr.enable ''
      cp ${herdrIntegration}/.claude/hooks/herdr-agent-state.sh $out/herdr-agent-state.sh
    ''}
  '';

  codexSkills = pkgs.runCommand "codex-skills" { } ''
    ${lib.getExe cfg.package} generate-skills \
      --source ${cfg.source} \
      --provider codex \
      ${extraSkillArgs} \
      --output $out
  '';

  claudeSkills = pkgs.runCommand "claude-skills" { } ''
    ${lib.getExe cfg.package} generate-skills \
      --source ${cfg.source} \
      --provider claude \
      ${extraSkillArgs} \
      --output $out
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
      description = "agent-harness source tree used for rendering harness assets.";
    };

    codex.enable = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = "Whether to install Codex harness files.";
    };

    claude.enable = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = "Whether to install Claude harness files.";
    };

    skills.extra = lib.mkOption {
      type = lib.types.attrsOf (lib.types.either lib.types.path lib.types.package);
      default = { };
      description = "Additional skill directories keyed by installed skill name.";
    };

    herdr.enable = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = "Whether to install Herdr session-reporting hooks.";
    };

    herdr.package = lib.mkOption {
      type = lib.types.nullOr lib.types.package;
      default = null;
      description = "Herdr package used to generate upstream integration artifacts.";
    };
  };

  config = lib.mkIf cfg.enable {
    assertions = [
      {
        assertion = !cfg.herdr.enable || cfg.herdr.package != null;
        message = "programs.agent-harness.herdr.package must be set when Herdr integration is enabled.";
      }
    ];

    home = {
      packages = [ cfg.package ];

      file = lib.mkMerge [
        (lib.mkIf cfg.codex.enable {
          ".codex/AGENTS.md".source = "${cfg.source}/agents/AGENTS.md";
          ".codex/hooks".source = "${cfg.source}/codex/hooks";
          ".codex/hooks.json".source = codexHooks;
          ".codex/rules/default.rules".source = codexRules;
          ".codex/skills".source = codexSkills;
        })
        (lib.mkIf cfg.claude.enable {
          ".claude/CLAUDE.md".source = "${cfg.source}/agents/AGENTS.md";
          ".claude/hooks".source = claudeHooks;
          ".claude/settings.json".source = claudeSettings;
          ".claude/skills".source = claudeSkills;
          ".claude/statusline".source = "${cfg.source}/claude/statusline";
        })
        (lib.mkIf (cfg.codex.enable && cfg.herdr.enable) {
          ".codex/herdr-agent-state.sh".source = "${herdrIntegration}/.codex/herdr-agent-state.sh";
        })
      ];

      activation.agentHarnessCodexConfig = lib.mkIf cfg.codex.enable (
        lib.hm.dag.entryAfter [ "writeBoundary" ] ''
          ${lib.getExe cfg.package} sync-codex-config \
            --source ${codexConfigSource} \
            --target "$HOME/.codex/config.toml"
        ''
      );
    };
  };
}
