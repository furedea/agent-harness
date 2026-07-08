{
  description = "";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-25.11-darwin";
  };

  outputs =
    {
      self,
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
