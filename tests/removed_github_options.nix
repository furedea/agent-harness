{
  agentHarnessModule,
  home-manager,
  pkgs,
  source,
}:
let
  inherit (pkgs) lib;

  configure =
    agentHarness:
    (home-manager.lib.homeManagerConfiguration {
      inherit pkgs;
      modules = [
        agentHarnessModule
        {
          home = {
            homeDirectory = "/Users/test";
            stateVersion = "25.11";
            username = "test";
          };
          programs.agent-harness = {
            enable = true;
            inherit source;
          }
          // agentHarness;
        }
      ];
    }).config;

  # Home Manager throws on evaluation when a module assertion fails, so the migration message
  # surfaces as the evaluation error.
  isRejected = config: !(builtins.tryEval config.home.username).success;
  withToken = configure { github.agentToken.enable = true; };
  withoutToken = configure { };
in
assert lib.assertMsg (isRejected withToken) "a removed GitHub option must fail evaluation";
assert lib.assertMsg (!isRejected withoutToken) "a configuration without GitHub options must pass";
assert lib.assertMsg (
  !(withoutToken.launchd.agents ? agent-harness-github-app-token)
) "the harness must not schedule a token refresh job";
pkgs.emptyFile
