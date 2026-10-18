{ lib }:
let
  githubDomains = [
    "github.com"
    "api.github.com"
  ];
  # git exclusions stay as the source lists them: signed agent commits exclude git, unsigned ones do not.
  sandboxedCommands = [ "gh *" ];
  # gh reads these files from GH_CONFIG_DIR; checking them catches globs that target the token file.
  configFiles = [
    "hosts.yml"
    "config.yml"
  ];

  readRulePattern =
    rule:
    let
      matched = builtins.match "Read\\((.*)\\)" rule;
    in
    if matched == null then [ ] else matched;

  readDenyPatterns =
    settings: secretPathPolicy:
    lib.concatMap readRulePattern (settings.permissions.deny or [ ])
    ++ (settings.sandbox.filesystem.denyRead or [ ])
    ++ map (rule: rule.pattern) (
      lib.filter (rule: builtins.elem "read" rule.access) (secretPathPolicy.rules or [ ])
    );

  globRegex =
    homeDirectory: pattern:
    let
      absolute =
        if lib.hasPrefix "~/" pattern then
          homeDirectory + lib.removePrefix "~" pattern
        else if lib.hasPrefix "//" pattern then
          lib.removePrefix "/" pattern
        else
          pattern;
      escaped = lib.escapeRegex absolute;
      wildcards =
        lib.replaceStrings [ "\\*\\*/" "\\*\\*" "\\*" "\\?" ] [ "(.*/)?" ".*" "[^/]*" "[^/]" ]
          escaped;
      # Relative patterns may match below any directory, so treat them as matching anywhere.
      anchored = if lib.hasPrefix "/" absolute then wildcards else "(.*/)?${wildcards}";
    in
    "${anchored}(/.*)?";

  pathAndAncestors =
    path:
    let
      segments = lib.filter (segment: segment != "") (lib.splitString "/" path);
    in
    map (count: "/" + lib.concatStringsSep "/" (lib.take count segments)) (
      lib.range 1 (builtins.length segments)
    );
in
{
  /**
    Claude settings that let gh run inside the sandbox and gh and git push
    authenticate with the token stored in `configDir`.
  */
  applyToClaudeSettings =
    configDir: settings:
    let
      sandbox = settings.sandbox or { };
      allowedDomains = sandbox.network.allowedDomains or [ ];
    in
    lib.recursiveUpdate settings {
      env.GH_CONFIG_DIR = configDir;
      sandbox = {
        enableWeakerNetworkIsolation = true;
        network.allowedDomains = lib.unique (allowedDomains ++ githubDomains);
      }
      // lib.optionalAttrs (sandbox ? excludedCommands) {
        excludedCommands = lib.subtractLists sandboxedCommands sandbox.excludedCommands;
      };
    };

  /**
    Claude settings that hide the GitHub App private key from Claude Code tools and the sandbox.
  */
  denyPrivateKeyRead =
    privateKeyPath: settings:
    lib.recursiveUpdate settings {
      # Read rules need a leading // for absolute paths; sandbox paths use a single /.
      permissions.deny = lib.unique (
        (settings.permissions.deny or [ ]) ++ [ "Read(/${privateKeyPath})" ]
      );
      sandbox.filesystem.denyRead = lib.unique (
        (settings.sandbox.filesystem.denyRead or [ ]) ++ [ privateKeyPath ]
      );
    };

  /**
    Codex settings that point shell commands at the token in `configDir` and make the guarded
    permissions profile the default: it reads everything except the normal gh token, writes the
    workspace, temporary directories, and the absolute paths in Claude's `allowWrite`, and allows
    network so git push and gh work inside the sandbox.
  */
  applyToCodexSettings =
    { configDir, claudeSettings }:
    settings:
    let
      # Relative allowWrite entries name Claude's working directory, which :workspace_roots covers.
      cacheWrites = lib.filter (path: lib.hasPrefix "/" path || lib.hasPrefix "~/" path) (
        claudeSettings.sandbox.filesystem.allowWrite or [ ]
      );
    in
    lib.recursiveUpdate settings {
      shell_environment_policy.set.GH_CONFIG_DIR = configDir;
      default_permissions = "guarded";
      permissions.guarded = {
        filesystem = lib.genAttrs cacheWrites (_: "write") // {
          ":root" = "read";
          ":tmpdir" = "write";
          ":slash_tmp" = "write";
          ":workspace_roots"."." = "write";
          "~/.config/gh/hosts.yml" = "deny";
        };
        network.enabled = true;
      };
    };

  /**
    Codex settings that hide the GitHub App private key in the guarded permissions profile.
  */
  denyCodexPrivateKeyRead =
    privateKeyPath: settings:
    lib.recursiveUpdate settings {
      permissions.guarded.filesystem.${privateKeyPath} = "deny";
    };

  /**
    Read deny patterns that would hide the token files in `configDir` from the sandbox.
  */
  readDenialsCovering =
    {
      homeDirectory,
      configDir,
      settings,
      secretPathPolicy,
    }:
    let
      candidates = pathAndAncestors configDir ++ map (file: "${configDir}/${file}") configFiles;
      covers =
        pattern:
        builtins.any (path: builtins.match (globRegex homeDirectory pattern) path != null) candidates;
    in
    lib.filter covers (readDenyPatterns settings secretPathPolicy);
}
