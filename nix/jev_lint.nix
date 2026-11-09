# mizchi/jev-lint 0.7.0 with every npm dependency fixed by nix/jev-lint/package-lock.json.
# The npm tarball ships no lockfile, so the repository keeps one for this wrapper package.
# To bump: edit nix/jev-lint/package.json, run
#   npm install --package-lock-only --ignore-scripts --prefix nix/jev-lint
# and replace npmDepsHash with the hash that `nix build .#jev-lint` reports.
{
  lib,
  stdenv,
  autoPatchelfHook,
  ast-grep,
  buildNpmPackage,
  makeWrapper,
  nodejs_24,
}:
buildNpmPackage {
  pname = "jev-lint";
  version = (builtins.fromJSON (builtins.readFile ./jev-lint/package.json)).dependencies.jev-lint;
  src = ./jev-lint;

  nodejs = nodejs_24;
  npmDepsHash = lib.fakeHash;
  # The platform ast-grep binary comes from the locked optional dependency; no install script may
  # download anything.
  npmFlags = [ "--ignore-scripts" ];
  dontNpmBuild = true;

  nativeBuildInputs = [
    makeWrapper
  ]
  ++ lib.optionals stdenv.hostPlatform.isLinux [ autoPatchelfHook ];
  buildInputs = lib.optionals stdenv.hostPlatform.isLinux [ stdenv.cc.cc.lib ];

  installPhase = ''
    runHook preInstall

    mkdir -p $out/lib/jev-lint $out/bin
    cp -R node_modules package.json $out/lib/jev-lint/
    makeWrapper ${lib.getExe nodejs_24} $out/bin/jev-lint \
      --add-flags $out/lib/jev-lint/node_modules/jev-lint/dist/cli.js \
      --prefix PATH : ${lib.makeBinPath [ ast-grep ]}:$out/lib/jev-lint/node_modules/.bin

    runHook postInstall
  '';

  meta = {
    description = "jev-lint pinned with its npm dependencies";
    mainProgram = "jev-lint";
  };
}
