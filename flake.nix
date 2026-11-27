{
  description = "";

  inputs = {
    home-manager = {
      url = "github:nix-community/home-manager/release-26.05";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-26.05-darwin";
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
      systems = [
        "x86_64-linux"
        "aarch64-darwin"
      ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
      pkgsFor = forAllSystems (system: import nixpkgs { inherit system; });
      completeSource = ./tests/fixtures/complete-source;
    in
    {
      packages = forAllSystems (
        system:
        let
          pkgs = pkgsFor.${system};
        in
        {
          default = pkgs.rustPlatform.buildRustPackage {
            pname = "agent-harness";
            inherit ((builtins.fromTOML (builtins.readFile ./Cargo.toml)).package) version;
            src = ./.;

            cargoLock.lockFile = ./Cargo.lock;
            nativeCheckInputs = [
              pkgs.nodejs_22
              pkgs.python3
              pkgs.git
            ];

            postInstall = ''
              mkdir -p $out/share/agent-harness
              cp -R profiles $out/share/agent-harness/
            '';

            meta.mainProgram = "agent-harness";
          };
        }
      );

      homeManagerModules.default = import ./nix/home_manager_module.nix { inherit self; };

      lib = forAllSystems (
        system:
        import ./nix/lib.nix {
          agentHarness = self.packages.${system}.default;
          pkgs = pkgsFor.${system};
        }
      );

      checks = forAllSystems (
        system:
        let
          pkgs = pkgsFor.${system};
        in
        {
          home-manager-minimal = import ./tests/home_manager_module.nix {
            agentHarnessModule = self.homeManagerModules.default;
            inherit home-manager pkgs;
            source = completeSource;
          };

          home-manager-pure-eval = import ./tests/home_manager_pure_eval.nix {
            agentHarnessModule = self.homeManagerModules.default;
            inherit home-manager pkgs;
            tree = completeSource;
          };

          removed-github-options = import ./tests/removed_github_options.nix {
            agentHarnessModule = self.homeManagerModules.default;
            inherit home-manager pkgs;
            source = completeSource;
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
        }
      );

      devShells = forAllSystems (
        system:
        let
          pkgs = pkgsFor.${system};
          # Must match cargo-dist-version in Cargo.toml and CARGO_DIST_VERSION in release.yml.
          cargoDist = pkgs.cargo-dist.overrideAttrs (
            finalAttrs: _: {
              version = "0.30.2";
              src = pkgs.fetchFromGitHub {
                owner = "axodotdev";
                repo = "cargo-dist";
                tag = "v${finalAttrs.version}";
                hash = "sha256-1stUmm7rtNB2z2srOzDvQ9QaGsS0CySBOHt118vmJoM=";
              };
              cargoHash = "sha256-Il5PVJHoNdifqUcXxKR+j+Lgga0kIsl7IJp9oGanZ+c=";
              cargoDeps = pkgs.rustPlatform.fetchCargoVendor {
                inherit (finalAttrs) src;
                hash = finalAttrs.cargoHash;
              };
            }
          );
          toolPackages =
            with pkgs;
            [
              bats
              cargo
              cargo-deny
              cargo-machete
              clippy
              commitlint
              deadnix
              jq
              ls-lint
              nixfmt
              nodejs_22
              rustc
              rustfmt
              statix
            ]
            # lefthook before 2.1.6 runs hooks under a pty, which agent sandboxes deny.
            ++ [ nixpkgs-unstable.legacyPackages.${system}.lefthook ];
        in
        {
          default = pkgs.mkShell { packages = toolPackages ++ [ cargoDist ]; };

          # CI does not run dist (release.yml installs its own), and building cargoDist from source takes
          # about seven minutes in every job.
          ci = pkgs.mkShell { packages = toolPackages; };
        }
      );
    };
}
