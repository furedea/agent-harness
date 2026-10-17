{
  agentHarnessModule,
  home-manager,
  pkgs,
  linuxPkgs,
  source,
}:
let
  inherit (pkgs) lib;

  configDir = "/home/test/.config/agent-harness/gh";
  privateKeyPath = "/home/test/.config/agent-harness/github_app.pem";
  sourceSettings = {
    permissions.deny = [ "Read(~/.config/gh/hosts.yml)" ];
    sandbox.excludedCommands = [
      "gh *"
      "git push *"
    ];
  };

  configure =
    hostPkgs: agentHarness:
    (home-manager.lib.homeManagerConfiguration {
      pkgs = hostPkgs;
      modules = [
        agentHarnessModule
        {
          home = {
            homeDirectory = "/home/test";
            stateVersion = "25.11";
            username = "test";
          };
          programs.agent-harness = lib.recursiveUpdate {
            enable = true;
            inherit source;
            # The flake only builds the darwin package; evaluation needs just an executable path.
            package = hostPkgs.writeShellScriptBin "agent-harness" "";
            claude.settings = sourceSettings;
          } agentHarness;
        }
      ];
    }).config;
  isRejected = config: !(builtins.tryEval config.home.username).success;
  composedSettings = config: config.programs.agent-harness.claude.composedSettings;

  appToken = {
    enable = true;
    source = "app";
    app = {
      id = 12345;
      inherit privateKeyPath;
    };
  };
  refreshArguments =
    installationArguments:
    [
      "github-app-token"
      "refresh"
      "--app-id"
      "12345"
    ]
    ++ installationArguments
    ++ [
      "--private-key"
      privateKeyPath
      "--config-dir"
      configDir
    ];

  pat = configure pkgs {
    github.agentToken = {
      enable = true;
      source = "pat";
    };
  };
  defaultSource = configure pkgs { github.agentToken.enable = true; };
  darwinApp = configure pkgs { github.agentToken = appToken; };
  darwinAgent = darwinApp.launchd.agents.agent-harness-github-app-token;
  darwinAppSettings = composedSettings darwinApp;
  explicitInstallation = configure pkgs {
    github.agentToken = lib.recursiveUpdate appToken { app.installationId = 678; };
  };
  linuxApp = configure linuxPkgs { github.agentToken = appToken; };
  linuxService = linuxApp.systemd.user.services.agent-harness-github-app-token;
  linuxTimer = linuxApp.systemd.user.timers.agent-harness-github-app-token;
  relativeKey = configure pkgs {
    github.agentToken = lib.recursiveUpdate appToken { app.privateKeyPath = "github_app.pem"; };
  };
in
assert lib.assertMsg (
  composedSettings pat == composedSettings defaultSource
) "the PAT source must stay the default and render unchanged";
assert lib.assertMsg (
  !(pat.launchd.agents ? agent-harness-github-app-token)
) "the PAT source must not install a refresh job";
assert lib.assertMsg (
  darwinAgent.enable
  && lib.tail darwinAgent.config.ProgramArguments == refreshArguments [ ]
  && darwinAgent.config.StartInterval == 3000
  && darwinAgent.config.RunAtLoad
) "the App source must refresh every 50 minutes and at load through launchd";
assert lib.assertMsg (
  lib.tail explicitInstallation.launchd.agents.agent-harness-github-app-token.config.ProgramArguments
  == refreshArguments [
    "--installation-id"
    "678"
  ]
) "the App source must pass a configured installation ID";
assert lib.assertMsg (
  darwinAppSettings.permissions.deny == [
    "Read(~/.config/gh/hosts.yml)"
    "Read(/${privateKeyPath})"
  ]
  && darwinAppSettings.sandbox.filesystem.denyRead == [ privateKeyPath ]
) "the App private key must be unreadable to Claude tools and sandboxed commands";
assert lib.assertMsg (
  darwinAppSettings.env.GH_CONFIG_DIR == configDir
  && darwinAppSettings.sandbox.excludedCommands == [ "git push *" ]
) "the App source must share the agent token settings";
assert lib.assertMsg (
  lib.hasInfix "github-app-token refresh --app-id 12345" (toString linuxService.Service.ExecStart)
  && linuxService.Install.WantedBy == [ "default.target" ]
  && linuxTimer.Timer.OnCalendar == "*:0/50"
  && linuxTimer.Timer.Persistent
) "the App source must refresh at login and every 50 minutes through systemd";
assert lib.assertMsg (isRejected relativeKey) "a relative App private key path must be rejected";
pkgs.emptyFile
