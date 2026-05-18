{ self }:
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.programs.agent-harness;

  claudeSettings = pkgs.runCommand "claude-settings.json" { } ''
    ${lib.getExe cfg.package} generate-claude-settings \
      --source ${cfg.source} \
      --out $out
  '';

  codexConfigSource = pkgs.runCommand "codex-config-source.toml" { } ''
    ${lib.getExe cfg.package} generate-codex-config-source \
      --source ${cfg.source} \
      --out $out
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
  };

  config = lib.mkIf cfg.enable {
    home = {
      packages = [ cfg.package ];

      file = lib.mkMerge [
        (lib.mkIf cfg.codex.enable {
          ".codex/AGENTS.md".source = "${cfg.source}/agents/AGENTS.md";
          ".codex/hooks".source = "${cfg.source}/codex/hooks";
          ".codex/skills".source = "${cfg.source}/agents/skills";
        })
        (lib.mkIf cfg.claude.enable {
          ".claude/CLAUDE.md".source = "${cfg.source}/agents/AGENTS.md";
          ".claude/hooks".source = "${cfg.source}/agents/hooks";
          ".claude/settings.json".source = claudeSettings;
          ".claude/skills".source = "${cfg.source}/agents/skills";
          ".claude/statusline".source = "${cfg.source}/claude/statusline";
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
