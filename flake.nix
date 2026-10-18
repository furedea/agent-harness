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
    in
    {
      packages.${system}.default = pkgs.rustPlatform.buildRustPackage {
        pname = "agent-harness";
        inherit ((builtins.fromTOML (builtins.readFile ./Cargo.toml)).package) version;
        src = ./.;

        cargoLock.lockFile = ./Cargo.lock;
        # Tests generate throwaway RSA keys with the openssl CLI instead of committing key material.
        nativeCheckInputs = [ pkgs.openssl ];

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
        home-manager-minimal = import ./tests/home_manager_module.nix {
          agentHarnessModule = self.homeManagerModules.default;
          inherit home-manager pkgs;
          source = ./tests/fixtures/complete-source;
        };

        agent-github-token = import ./tests/agent_github_token.nix {
          agentHarnessModule = self.homeManagerModules.default;
          inherit home-manager pkgs;
          source = ./tests/fixtures/complete-source;
        };

        agent-github-app-token = import ./tests/agent_github_app_token.nix {
          agentHarnessModule = self.homeManagerModules.default;
          inherit home-manager pkgs;
          linuxPkgs = import nixpkgs { system = "x86_64-linux"; };
          source = ./tests/fixtures/complete-source;
        };

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
        packages =
          with pkgs;
          [
            cargo
            cargo-deny
            cargo-machete
            clippy
            commitlint
            deadnix
            ls-lint
            nixfmt-rfc-style
            nodejs_22
            openssl
            rustc
            rustfmt
            statix
          ]
          # lefthook before 2.1.6 runs hooks under a pty, which agent sandboxes deny.
          ++ [ nixpkgs-unstable.legacyPackages.${system}.lefthook ];
      };
    };
}
