{
  description = "";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-25.11-darwin";
    codex-cli-nix.url = "github:sadjow/codex-cli-nix/7f0f3802287581e04501e2fea26b56d63df18ebd";
  };

  outputs =
    {
      self,
      nixpkgs,
      codex-cli-nix,
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
          cp -R agents claude codex $out/share/agent-harness/
        '';

        meta.mainProgram = "agent-harness";
      };

      homeManagerModules.default = import ./nix/home_manager_module.nix { inherit self; };

      devShells.${system}.default = pkgs.mkShell {
        packages = with pkgs; [
          cargo
          cargo-deny
          cargo-machete
          clippy
          codex-cli-nix.packages.${system}.default
          commitlint
          deadnix
          lefthook
          ls-lint
          nixfmt-rfc-style
          rustc
          rustfmt
          statix
          uv
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
