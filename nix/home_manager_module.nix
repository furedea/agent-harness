{ self }:
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.programs.agent-harness;
  providers = builtins.fromJSON (builtins.readFile ./providers.json);
  providerOptions = lib.genAttrs (map (provider: provider.id) providers) (
    id:
    let
      provider = lib.findFirst (provider: provider.id == id) null providers;
    in
    {
      enable = lib.mkOption {
        type = lib.types.bool;
        default = true;
        description = provider.option_description;
      };
    }
    // lib.mapAttrs (
      _: option:
      lib.mkOption {
        type = lib.types.str;
        inherit (option) default description;
      }
    ) provider.extra_options
  );
  providerFiles = map (
    provider:
    lib.mkIf cfg.${provider.id}.enable (
      lib.mapAttrs (path: link: {
        source =
          if link ? skills_provider then
            providerSkills link.skills_provider
          else
            "${renderedHarness}/${path}";
      }) provider.linked_paths
    )
  ) providers;
  syncActivation =
    provider: sync:
    let
      target = lib.replaceStrings [ "\${profile}" ] [ (cfg.${provider.id}.profile or "") ] sync.target;
    in
    lib.nameValuePair sync.activation (
      lib.mkIf cfg.${provider.id}.enable (
        lib.hm.dag.entryAfter [ (sync.after or "linkGeneration") ] ''
          ${lib.getExe cfg.package} ${sync.command} \
            --source ${renderedHarness}/${sync.source} \
            --target "$HOME/${target}"
        ''
      )
    );
  # Each provider's config, and the MCP server file of its own when it has one.
  providerActivations = lib.listToAttrs (
    lib.concatMap (
      provider:
      map (syncActivation provider) (
        builtins.filter (sync: sync != null) [
          provider.sync
          (provider.mcp_sync or null)
        ]
      )
    ) providers
  );
  # Evaluation reads `tree` (a plain path) so it never builds `cfg.source`; the built
  # `cfg.source` only supplies files copied at build time. Path concatenation (`+`) keeps
  # the value a path, whereas string interpolation would copy it to the store.
  sourceTree = cfg.source.tree or cfg.source;
  sourceHasProfiles = builtins.pathExists (sourceTree + "/profiles/${cfg.profile}");
  evalProfileSource =
    if sourceHasProfiles then sourceTree + "/profiles/${cfg.profile}" else sourceTree;
  profileSource = if sourceHasProfiles then "${cfg.source}/profiles/${cfg.profile}" else cfg.source;

  agentsMd = if cfg.agentsMd == null then "${profileSource}/AGENTS.md" else cfg.agentsMd;
  commandPermissions =
    if cfg.commandPermissions == null then
      "${profileSource}/command_permissions.json"
    else
      cfg.commandPermissions;

  claudeBaseSettings = builtins.fromJSON (
    builtins.readFile (evalProfileSource + "/claude/settings.json")
  );
  claudeSourceSettings = lib.recursiveUpdate claudeBaseSettings cfg.claude.settings;
  claudeSettings = pkgs.writeText "agent-harness-claude-settings.json" (
    builtins.toJSON claudeSourceSettings
  );

  codexFormat = pkgs.formats.toml { };
  codexBaseSettings = builtins.fromTOML (
    builtins.readFile (evalProfileSource + "/codex/config.toml")
  );
  codexSourceSettings = lib.recursiveUpdate codexBaseSettings cfg.codex.settings;
  codexSettings = codexFormat.generate "agent-harness-codex-settings.toml" codexSourceSettings;

  antigravityBaseSettings =
    if builtins.pathExists (evalProfileSource + "/antigravity/settings.json") then
      builtins.fromJSON (builtins.readFile (evalProfileSource + "/antigravity/settings.json"))
    else
      { };
  antigravitySourceSettings = lib.recursiveUpdate antigravityBaseSettings cfg.antigravity.settings;
  antigravitySettings = pkgs.writeText "agent-harness-antigravity-settings.json" (
    builtins.toJSON antigravitySourceSettings
  );

  # Servers are replaced by name: an option entry replaces the source's entry of that name whole.
  # Every other root key of the source stays, so the harness still rejects a malformed file.
  sourceMcpServersPath = evalProfileSource + "/mcp_servers.json";
  sourceMcpServers =
    if builtins.pathExists sourceMcpServersPath then
      builtins.fromJSON (builtins.readFile sourceMcpServersPath)
    else
      { servers = { }; };
  mcpServers = pkgs.writeText "agent-harness-mcp-servers.json" (
    builtins.toJSON (
      sourceMcpServers // { servers = (sourceMcpServers.servers or { }) // cfg.mcpServers; }
    )
  );

  composedSource = pkgs.runCommand "agent-harness-${cfg.profile}-source" { } ''
    cp -R ${lib.escapeShellArg "${profileSource}/."} "$out"
    chmod -R u+w "$out"
    install -m 0644 ${lib.escapeShellArg "${agentsMd}"} "$out/AGENTS.md"
    install -m 0644 ${lib.escapeShellArg "${commandPermissions}"} \
      "$out/command_permissions.json"
    install -m 0644 ${claudeSettings} "$out/claude/settings.json"
    install -m 0644 ${codexSettings} "$out/codex/config.toml"
    mkdir -p "$out/antigravity"
    install -m 0644 ${antigravitySettings} "$out/antigravity/settings.json"
    install -m 0644 ${mcpServers} "$out/mcp_servers.json"
  '';

  namedSourceArgs =
    option: flag:
    lib.escapeShellArgs (
      lib.concatLists (
        lib.mapAttrsToList (name: source: [
          flag
          "${name}=${source}"
        ]) option
      )
    );
  skillArgs = namedSourceArgs cfg.skills "--extra-skill";
  hookArgs = namedSourceArgs cfg.hooks "--extra-hook";

  renderedHarness = pkgs.runCommand "agent-harness-rendered" { } ''
    ${lib.getExe cfg.package} --profile ${cfg.profile} install \
      --source ${composedSource} \
      --prefix "$out" \
      ${hookArgs}
  '';

  providerSkills =
    provider:
    pkgs.runCommand "${provider}-skills" { } ''
      ${lib.getExe cfg.package} --profile ${cfg.profile} generate-skills \
        --source ${composedSource} \
        --provider ${provider} \
        ${skillArgs} \
        --output "$out"
    '';
in
{
  imports = [
    (lib.mkRemovedOptionModule [ "programs" "agent-harness" "github" ] ''
      agent-harness no longer composes an agent GitHub token or refreshes one (ADR-0032).
      Declare the wiring in the source settings instead: Claude env.GH_CONFIG_DIR, GitHub hosts
      in sandbox.network.allowedDomains, sandbox.enableWeakerNetworkIsolation, gh outside
      sandbox.excludedCommands, and Read denials of the App private key; Codex
      shell_environment_policy.set.GH_CONFIG_DIR and a guarded permissions profile. Mint and
      refresh the token outside agent-harness.
    '')
  ];

  # Provider options (`enable`, extra options) come from providers.json and merge into
  # the explicit per-provider settings below.
  options.programs.agent-harness = lib.recursiveUpdate {
    enable = lib.mkEnableOption "agent harness";

    package = lib.mkOption {
      type = lib.types.package;
      inherit (self.packages.${pkgs.system}) default;
      description = "agent-harness package to use.";
    };

    source = lib.mkOption {
      type = lib.types.path;
      default = self;
      description = ''
        Profile collection or complete flat agent-harness source tree. A built source may
        carry `passthru.tree`, a path to its unbuilt tree: evaluation then reads settings and
        profiles from `tree` so it never builds the source. `tree`
        must hold the same content as the built output for every file read at evaluation time.
      '';
    };

    profile = lib.mkOption {
      type = lib.types.enum [ "minimal" ];
      default = "minimal";
      description = "Built-in harness profile used as the composition base.";
    };

    agentsMd = lib.mkOption {
      type = lib.types.nullOr lib.types.path;
      default = null;
      description = "Optional AGENTS.md source shared by Codex and Claude Code.";
    };

    commandPermissions = lib.mkOption {
      type = lib.types.nullOr lib.types.path;
      default = null;
      description = "Optional provider-neutral command permissions JSON source.";
    };

    mcpServers = lib.mkOption {
      type = lib.types.attrsOf (lib.types.attrsOf lib.types.anything);
      default = { };
      example = lib.literalExpression ''
        {
          github = {
            command = "''${pkgs.github-mcp-server}/bin/github-mcp-server";
            args = [ "stdio" ];
          };
          docs.url = "https://docs.example.com/mcp";
        }
      '';
      description = ''
        MCP servers keyed by name, each with `command` (and optional `args` and `env`) or `url`.
        An entry replaces the source `mcp_servers.json` entry of the same name. Values are written
        to provider configs as they are, so never put a credential here.
      '';
    };

    skills = lib.mkOption {
      type = lib.types.attrsOf (lib.types.either lib.types.path lib.types.package);
      default = { };
      description = "Skill directories keyed by installed skill name.";
    };

    hooks = lib.mkOption {
      type = lib.types.attrsOf (lib.types.either lib.types.path lib.types.package);
      default = { };
      description = "External hook bundle directories keyed by installed bundle name.";
    };

    codex = {
      settings = lib.mkOption {
        type = lib.types.attrsOf lib.types.anything;
        default = { };
        description = "Codex settings recursively merged over the selected profile.";
      };
    };

    antigravity = {
      settings = lib.mkOption {
        type = lib.types.attrsOf lib.types.anything;
        default = { };
        description = "Antigravity CLI settings recursively merged over the selected profile.";
      };
    };

    claude = {
      settings = lib.mkOption {
        type = lib.types.attrsOf lib.types.anything;
        default = { };
        description = "Claude Code settings recursively merged over the selected profile.";
      };
    };
  } providerOptions;

  config = lib.mkIf cfg.enable {
    home = {
      packages = [ cfg.package ];

      file = lib.mkMerge providerFiles;

      activation = providerActivations;
    };
  };
}
