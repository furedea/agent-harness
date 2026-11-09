{
  agentHarnessModule,
  home-manager,
  pkgs,
  tree,
}:
let
  # A built source that cannot be realised during evaluation: reading it would be
  # import-from-derivation. Only `passthru.tree` may be read at evaluation time.
  builtSource = pkgs.runCommand "agent-harness-unbuilt-source" { passthru = { inherit tree; }; } ''
    cp -R ${tree} "$out"
  '';
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
          source = builtSource;
          agentsMd = tree + "/AGENTS.md";
          commandPermissions = tree + "/command_permissions.json";
        };
      }
    ];
  };
in
configuration.activationPackage
