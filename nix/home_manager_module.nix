{ self }:
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.programs.agent-harness;

  rendered = pkgs.runCommand "agent-harness-rendered" { } ''
    ${lib.getExe cfg.package} render \
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
          ".codex/AGENTS.md".source = "${rendered}/codex/AGENTS.md";
          ".codex/hooks".source = "${rendered}/codex/hooks";
          ".codex/skills".source = "${rendered}/codex/skills";
        })
        (lib.mkIf cfg.claude.enable {
          ".claude/CLAUDE.md".source = "${rendered}/claude/CLAUDE.md";
          ".claude/hooks".source = "${rendered}/claude/hooks";
          ".claude/settings.json".source = "${rendered}/claude/settings.json";
          ".claude/skills".source = "${rendered}/claude/skills";
          ".claude/statusline".source = "${rendered}/claude/statusline";
        })
      ];

      activation.agentHarnessCodexConfig = lib.mkIf cfg.codex.enable (
        lib.hm.dag.entryAfter [ "writeBoundary" ] ''
          ${lib.getExe cfg.package} sync-codex-config \
            --source ${rendered}/codex/config-source.toml \
            --target "$HOME/.codex/config.toml"
        ''
      );
    };
  };
}
