{ self }:
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.programs.agent-harness;
  extraSkillArgs = lib.escapeShellArgs (
    lib.concatLists (
      lib.mapAttrsToList (name: source: [
        "--extra-skill"
        "${name}=${source}"
      ]) cfg.skills.extra
    )
  );
  extraHookArgs = lib.escapeShellArgs (
    lib.concatLists (
      lib.mapAttrsToList (name: source: [
        "--extra-hook"
        "${name}=${source}"
      ]) cfg.hooks.extra
    )
  );
  herdrArgs = lib.optionals cfg.herdr.enable [
    "--enable-herdr"
    "--herdr-bin"
    (lib.getExe cfg.herdr.package)
  ];
  renderedHarnessArgs = lib.escapeShellArgs herdrArgs;

  renderedHarness = pkgs.runCommand "agent-harness-rendered" { } ''
    ${lib.getExe cfg.package} install \
      --source ${lib.escapeShellArg (toString cfg.source)} \
      --prefix "$out" \
      ${renderedHarnessArgs} \
      ${extraHookArgs}
  '';

  codexRules = pkgs.runCommand "codex-default.rules" { } ''
    ${lib.getExe cfg.package} generate-codex-rules \
      --source ${cfg.source} \
      --output $out
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

    hooks.extra = lib.mkOption {
      type = lib.types.attrsOf (lib.types.either lib.types.path lib.types.package);
      default = { };
      description = "External hook bundle directories keyed by installed bundle name.";
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
      {
        assertion = !cfg.herdr.enable || !(cfg.hooks.extra ? herdr);
        message = "Use either programs.agent-harness.herdr or hooks.extra.herdr, not both.";
      }
    ];

    home = {
      packages = [ cfg.package ];

      file = lib.mkMerge [
        (lib.mkIf cfg.codex.enable {
          ".codex/AGENTS.md".source = "${cfg.source}/agents/AGENTS.md";
          ".codex/hooks".source = "${renderedHarness}/.codex/hooks";
          ".codex/hooks.json".source = "${renderedHarness}/.codex/hooks.json";
          ".codex/rules/default.rules".source = codexRules;
          ".codex/skills".source = codexSkills;
        })
        (lib.mkIf cfg.claude.enable {
          ".claude/CLAUDE.md".source = "${cfg.source}/agents/AGENTS.md";
          ".claude/hooks".source = "${renderedHarness}/.claude/hooks";
          ".claude/settings.json".source = "${renderedHarness}/.claude/settings.json";
          ".claude/skills".source = claudeSkills;
          ".claude/statusline".source = "${cfg.source}/claude/statusline";
        })
        (lib.mkIf (cfg.codex.enable && cfg.herdr.enable) {
          ".codex/herdr-agent-state.sh".source = "${renderedHarness}/.codex/herdr-agent-state.sh";
        })
      ];

      activation.agentHarnessCodexConfig = lib.mkIf cfg.codex.enable (
        lib.hm.dag.entryAfter [ "writeBoundary" ] ''
          ${lib.getExe cfg.package} sync-codex-config \
            --source ${renderedHarness}/.codex/config.toml \
            --target "$HOME/.codex/config.toml"
        ''
      );
    };
  };
}
