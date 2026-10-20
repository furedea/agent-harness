{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.programs.agent-harness;
  inherit (cfg.github) agentToken;
  inherit (agentToken) app;
  jobName = "agent-harness-github-app-token";
  # Installation tokens live for one hour. The job checks every 5 minutes and mints only when
  # under 20 minutes remain, so a late or skipped run still leaves time before expiry.
  checkIntervalSecs = 5 * 60;

  refreshCommand = [
    (lib.getExe cfg.package)
    "github-app-token"
    "refresh"
    "--app-id"
    (toString app.id)
  ]
  ++ lib.optionals (app.installationId != null) [
    "--installation-id"
    (toString app.installationId)
  ]
  ++ [
    "--private-key"
    app.privateKeyPath
    "--config-dir"
    agentToken.configDir
  ];
in
{
  options.programs.agent-harness.github.agentToken = {
    source = lib.mkOption {
      type = lib.types.enum [
        "pat"
        "app"
      ];
      default = "pat";
      description = ''
        Where the agent token comes from: "pat" for a token stored once with gh auth login,
        "app" for GitHub App installation tokens that a scheduled job refreshes.
      '';
    };

    app = {
      id = lib.mkOption {
        type = lib.types.ints.positive;
        description = "GitHub App ID.";
      };

      installationId = lib.mkOption {
        type = lib.types.nullOr lib.types.ints.positive;
        default = null;
        description = ''
          Installation to mint tokens for. Null uses the App's only installation and fails
          when the App is installed on several accounts.
        '';
      };

      privateKeyPath = lib.mkOption {
        type = lib.types.str;
        description = ''
          Absolute path of the App private key. Claude Code tools and sandboxed commands are
          denied read access to it.
        '';
      };
    };
  };

  config = lib.mkIf (cfg.enable && agentToken.enable && agentToken.source == "app") (
    lib.mkMerge [
      {
        assertions = [
          {
            assertion = lib.hasPrefix "/" app.privateKeyPath;
            message = "programs.agent-harness.github.agentToken.app.privateKeyPath must be an absolute path.";
          }
        ];
      }
      (lib.mkIf pkgs.stdenv.hostPlatform.isDarwin {
        launchd.agents.${jobName} = {
          enable = true;
          config = {
            ProgramArguments = refreshCommand;
            # launchd runs a missed interval once after wake.
            StartInterval = checkIntervalSecs;
            RunAtLoad = true;
            ProcessType = "Background";
            StandardOutPath = "${config.home.homeDirectory}/Library/Logs/${jobName}.log";
            StandardErrorPath = "${config.home.homeDirectory}/Library/Logs/${jobName}.log";
          };
        };
      })
      (lib.mkIf pkgs.stdenv.hostPlatform.isLinux {
        systemd.user = {
          services.${jobName} = {
            Unit.Description = "Refresh the agent-harness GitHub App token";
            Service = {
              Type = "oneshot";
              ExecStart = lib.escapeShellArgs refreshCommand;
            };
            Install.WantedBy = [ "default.target" ];
          };
          timers.${jobName} = {
            Unit.Description = "Check the agent-harness GitHub App token every 5 minutes";
            Timer = {
              # Wall-clock schedule with Persistent catches up after suspend, unlike monotonic timers.
              OnCalendar = "*:0/5";
              Persistent = true;
            };
            Install.WantedBy = [ "timers.target" ];
          };
        };
      })
    ]
  );
}
