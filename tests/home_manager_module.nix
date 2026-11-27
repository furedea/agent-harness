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
          # Replaces the source's `docs` server and adds one beside its `github` server.
          mcpServers = {
            docs.url = "https://docs.example.org/mcp";
            local = {
              command = "/nix/store/example-mcp/bin/example-mcp";
              args = [ "--stdio" ];
            };
          };
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
assert pkgs.lib.assertMsg (builtins.all hasHomeFile [
  ".pi/agent/skills"
  ".pi/agent/AGENTS.md"
]) "pi skills and instructions should be installed";
assert pkgs.lib.assertMsg (
  !hasHomeFile ".pi/agent/settings.json"
) "Pi settings should not be a Home Manager symlink";
assert pkgs.lib.assertMsg (hasActivation "agentHarnessPiConfig")
  "Pi settings should be materialized during activation";
assert pkgs.lib.assertMsg (hasActivation "agentHarnessDshConfig") "dsh profile should be synced";
assert pkgs.lib.assertMsg (builtins.all hasHomeFile [
  ".dsh/hooks"
  ".dsh/hooks.json"
  ".dsh/bridge"
  ".dsh/skills"
  ".dsh/AGENTS.md"
]) "dsh files should be installed";
assert pkgs.lib.assertMsg (hasActivation "agentHarnessAntigravityConfig")
  "Antigravity settings should be materialized during activation";
assert pkgs.lib.assertMsg (
  !hasHomeFile ".gemini/antigravity-cli/settings.json"
) "Antigravity settings should not be a Home Manager symlink";
assert pkgs.lib.assertMsg (builtins.all hasHomeFile [
  ".gemini/config/hooks"
  ".gemini/config/hooks.json"
  ".gemini/config/skills"
  ".gemini/config/AGENTS.md"
]) "Antigravity files should be installed";
let
  claudeSkills = configuration.config.home.file.".claude/skills".source;
  piSkills = configuration.config.home.file.".pi/agent/skills".source;
in
pkgs.runCommand "home-manager-minimal-check" { } ''
  plugin=${claudeSkills}/agent-harness/.claude-plugin/plugin.json
  grep -q '"github-mcp-server"' "$plugin"
  grep -q '"https://docs.example.org/mcp"' "$plugin"
  grep -q '"/nix/store/example-mcp/bin/example-mcp"' "$plugin"
  if grep -q 'docs.example.com' "$plugin"; then
    echo "the option should replace the source's docs server" >&2
    exit 1
  fi
  if [ -e ${piSkills}/agent-harness ]; then
    echo "pi skills should not carry the Claude MCP plugin" >&2
    exit 1
  fi
  ln -s ${configuration.activationPackage} "$out"
''
