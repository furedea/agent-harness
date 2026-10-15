{
  agentHarnessModule,
  home-manager,
  pkgs,
  source,
}:
let
  inherit (pkgs) lib;

  homeDirectory = "/Users/test";
  defaultConfigDir = "${homeDirectory}/.config/agent-harness/gh";
  sourceSettings = {
    env.EXISTING = "1";
    permissions.deny = [ "Read(~/.config/gh/hosts.yml)" ];
    sandbox = {
      network.allowedDomains = [
        "api.day.app"
        "github.com"
      ];
      excludedCommands = [
        "docker *"
        "gh *"
        "git commit *"
        "git push *"
      ];
    };
  };

  configure =
    agentHarness:
    (home-manager.lib.homeManagerConfiguration {
      inherit pkgs;
      modules = [
        agentHarnessModule
        {
          home = {
            inherit homeDirectory;
            stateVersion = "25.11";
            username = "test";
          };
          programs.agent-harness = lib.recursiveUpdate {
            enable = true;
            inherit source;
            claude.settings = sourceSettings;
          } agentHarness;
        }
      ];
    }).config;

  composedSettings = config: config.programs.agent-harness.claude.composedSettings;
  # Home Manager throws on evaluation when a module assertion fails.
  isRejected = config: !(builtins.tryEval config.home.username).success;
  sourceComposition = lib.recursiveUpdate (builtins.fromJSON (
    builtins.readFile (source + "/claude/settings.json")
  )) sourceSettings;

  disabled = configure { };
  enabled = configure { github.agentToken.enable = true; };
  enabledSettings = composedSettings enabled;
  customDir = configure {
    github.agentToken = {
      enable = true;
      configDir = "/opt/agent-gh";
    };
  };
  insideNormalGhDir = configure {
    github.agentToken = {
      enable = true;
      configDir = "${homeDirectory}/.config/gh";
    };
  };
  deniedBySecretPathPolicy = configure {
    github.agentToken = {
      enable = true;
      configDir = "${homeDirectory}/secrets/gh";
    };
  };
  deniedByDenyRead = configure {
    github.agentToken.enable = true;
    claude.settings.sandbox.filesystem.denyRead = [ "~/.config/agent-harness" ];
  };
  relativeDir = configure {
    github.agentToken = {
      enable = true;
      configDir = "agent-gh";
    };
  };
in
assert lib.assertMsg (
  composedSettings disabled == sourceComposition
) "disabled agent token must leave composed Claude settings unchanged";
assert lib.assertMsg (
  enabledSettings.env == {
    EXISTING = "1";
    GH_CONFIG_DIR = defaultConfigDir;
  }
) "agent token must point GH_CONFIG_DIR at the absolute default config directory";
assert lib.assertMsg (
  enabledSettings.sandbox.network.allowedDomains == [
    "api.day.app"
    "github.com"
    "api.github.com"
  ]
) "agent token must allow GitHub hosts once each";
assert lib.assertMsg enabledSettings.sandbox.enableWeakerNetworkIsolation
  "agent token must enable weaker network isolation for gh TLS verification";
assert lib.assertMsg (
  enabledSettings.sandbox.excludedCommands == [
    "docker *"
    "git commit *"
    "git push *"
  ]
) "agent token must run gh inside the sandbox and keep git push outside";
assert lib.assertMsg (
  enabledSettings.permissions.deny == [ "Read(~/.config/gh/hosts.yml)" ]
) "agent token must keep the normal gh token denied";
assert lib.assertMsg (
  !isRejected enabled
) "default agent token config directory must be readable inside the sandbox";
assert lib.assertMsg (
  (composedSettings customDir).env.GH_CONFIG_DIR == "/opt/agent-gh"
) "agent token must use a configured config directory";
assert lib.assertMsg (isRejected insideNormalGhDir)
  "a config directory covered by a Read deny must be rejected";
assert lib.assertMsg (isRejected deniedBySecretPathPolicy)
  "a config directory covered by the secret path policy must be rejected";
assert lib.assertMsg (isRejected deniedByDenyRead)
  "a config directory covered by sandbox denyRead must be rejected";
assert lib.assertMsg (isRejected relativeDir) "a relative config directory must be rejected";
pkgs.emptyFile
