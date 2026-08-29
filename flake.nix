{
  description = "";

  inputs = {
    home-manager = {
      url = "github:nix-community/home-manager/release-25.11";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-25.11-darwin";
  };

  outputs =
    {
      self,
      home-manager,
      nixpkgs,
      ...
    }:
    let
      system = "aarch64-darwin";
      pkgs = import nixpkgs { inherit system; };
    in
    {
      packages.${system}.default = pkgs.rustPlatform.buildRustPackage {
        pname = "agent-harness";
        inherit ((builtins.fromTOML (builtins.readFile ./Cargo.toml)).package) version;
        src = ./.;

        cargoLock.lockFile = ./Cargo.lock;

        postInstall = ''
          mkdir -p $out/share/agent-harness
          cp -R profiles $out/share/agent-harness/
        '';

        meta.mainProgram = "agent-harness";
      };

      homeManagerModules.default = import ./nix/home_manager_module.nix { inherit self; };

      lib.${system} = import ./nix/lib.nix {
        agentHarness = self.packages.${system}.default;
        inherit pkgs;
      };

      checks.${system} = {
        home-manager-minimal =
          (home-manager.lib.homeManagerConfiguration {
            inherit pkgs;
            modules = [
              self.homeManagerModules.default
              {
                home = {
                  homeDirectory = "/Users/test";
                  stateVersion = "25.11";
                  username = "test";
                };
                programs.agent-harness = {
                  enable = true;
                  source = ./tests/fixtures/complete-source;
                  agentsMd = ./tests/fixtures/complete-source/AGENTS.md;
                  commandPermissions = ./tests/fixtures/complete-source/command_permissions.json;
                };
              }
              (
                { config, ... }:
                let
                  claudeLinks = builtins.filter (path: pkgs.lib.hasPrefix ".claude/" path) (
                    builtins.attrNames config.home.file
                  );
                in
                {
                  assertions = [
                    {
                      assertion =
                        claudeLinks == [
                          ".claude/CLAUDE.md"
                          ".claude/hooks"
                          ".claude/skills"
                          ".claude/statusline"
                        ];
                      message = "only immutable Claude files should be Home Manager links";
                    }
                    {
                      assertion = builtins.hasAttr "agentHarnessClaudeSettings" config.home.activation;
                      message = "Claude settings should be materialized during activation";
                    }
                  ];
                }
              )
            ];
          }).activationPackage;

        skill-from-command = self.lib.${system}.buildSkillFromCommand {
          name = "example";
          command = [
            "${pkgs.coreutils}/bin/printf"
            "%s\\n"
            "---"
            "name: example"
            "description: Generated skill."
            "---"
          ];
        };

        hook-bundle-from-command =
          let
            installer = pkgs.writeShellApplication {
              name = "example-hook-installer";
              text = ''
                readonly HOOK_DIR="$HOME/.claude/hooks"

                mkdir -p "$HOOK_DIR"
                printf '%s\n' '#!/usr/bin/env bash' 'exit 0' > "$HOOK_DIR/example.sh"
                chmod +x "$HOOK_DIR/example.sh"
                printf '%s\n' \
                  "{\"hooks\":{\"PreToolUse\":[{\"hooks\":[{\"type\":\"command\",\"command\":\"$HOME/.claude/hooks/example.sh\"}]}]}}" \
                  > "$HOME/.claude/settings.json"
              '';
            };
          in
          self.lib.${system}.buildHookBundleFromCommands {
            name = "example";
            commands = [ [ (pkgs.lib.getExe installer) ] ];
          };
      };

      devShells.${system}.default = pkgs.mkShell {
        packages = with pkgs; [
          cargo
          cargo-deny
          cargo-machete
          clippy
          commitlint
          deadnix
          lefthook
          ls-lint
          nixfmt-rfc-style
          nodejs_22
          rustc
          rustfmt
          statix
        ];
      };
    };
}
