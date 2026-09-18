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
assert pkgs.lib.assertMsg (
  !hasHomeFile ".config/devin/config.json"
) "Devin config should not be a Home Manager symlink";
assert pkgs.lib.assertMsg (hasHomeFile ".devin/hooks")
  "Devin hook scripts should be installed as a Home Manager file";
assert pkgs.lib.assertMsg (hasActivation "agentHarnessDevinConfig")
  "Devin config should be materialized during activation";
assert pkgs.lib.assertMsg (
  !hasHomeFile ".hermes/config.yaml"
) "Hermes config should not be a Home Manager symlink";
assert pkgs.lib.assertMsg (hasHomeFile ".hermes/hooks")
  "Hermes hook scripts should be installed as a Home Manager file";
assert pkgs.lib.assertMsg (hasHomeFile ".hermes/hooks.json")
  "Hermes hook manifest should be installed as a Home Manager file";
assert pkgs.lib.assertMsg (hasHomeFile ".hermes/plugins/agent-harness-hooks")
  "Hermes bridge plugin should be installed as a Home Manager file";
assert pkgs.lib.assertMsg (hasActivation "agentHarnessHermesConfig")
  "Hermes config should be materialized during activation";
assert pkgs.lib.assertMsg (hasHomeFile ".pi/hooks")
  "pi hook scripts should be installed as a Home Manager file";
assert pkgs.lib.assertMsg (hasHomeFile ".pi/agent/hooks.json")
  "pi hook manifest should be installed as a Home Manager file";
assert pkgs.lib.assertMsg (hasHomeFile ".pi/agent/extensions/hook_bridge.ts")
  "pi bridge extension should be installed as a Home Manager file";
configuration.activationPackage
