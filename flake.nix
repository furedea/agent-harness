{
  description = "";

  inputs = {
    home-manager = {
      url = "github:nix-community/home-manager/release-25.11";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-25.11-darwin";
    nixpkgs-unstable.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
  };

  outputs =
    {
      self,
      home-manager,
      nixpkgs,
      nixpkgs-unstable,
      ...
    }:
    let
      system = "aarch64-darwin";
      pkgs = import nixpkgs { inherit system; };
      unstable = import nixpkgs-unstable { inherit system; };
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
                  agentsMd = ./profiles/furedea/agents/AGENTS.md;
                  claude.settings.model = "claude-opus-4-1";
                  commandPermissions = ./profiles/furedea/agents/command_permissions.json;
                  codex.settings.model = "gpt-5.5";
                  skills.adr = ./profiles/furedea/agents/skills/adr;
                };
              }
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
          unstable.uv
        ];

        env = {
          UV_MANAGED_PYTHON = "1";
        };

        shellHook = ''
          if [ -d .venv/bin ]; then
            export PATH="$PWD/.venv/bin:$PATH"
          fi
        '';
      };
    };
}
