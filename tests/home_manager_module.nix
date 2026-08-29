{
  agentHarnessModule,
  home-manager,
  pkgs,
  source,
}:
let
  configuration = home-manager.lib.homeManagerConfiguration {
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
          agentsMd = source + "/AGENTS.md";
          commandPermissions = source + "/command_permissions.json";
        };
      }
    ];
  };
  hasActivation = name: builtins.hasAttr name configuration.config.home.activation;
  hasHomeFile = path: builtins.hasAttr path configuration.config.home.file;
in
assert pkgs.lib.assertMsg (
  !hasHomeFile ".claude/settings.json"
) "Claude settings should not be a Home Manager symlink";
assert pkgs.lib.assertMsg (hasActivation "agentHarnessClaudeSettings")
  "Claude settings should be materialized during activation";
assert pkgs.lib.assertMsg (
  !hasHomeFile ".codex/config.toml"
) "Codex config should not be a Home Manager symlink";
assert pkgs.lib.assertMsg (hasActivation "agentHarnessCodexConfig")
  "Codex config should be materialized during activation";
configuration.activationPackage
