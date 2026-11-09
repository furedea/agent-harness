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
  providerActivations = lib.listToAttrs (
    map (
      provider:
      let
        inherit (provider) sync;
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
      )
    ) (builtins.filter (provider: provider.sync != null) providers)
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

  agentGithubToken = import ./agent_github_token.nix { inherit lib; };
  inherit (cfg.github) agentToken;
  agentTokenDenials = agentGithubToken.readDenialsCovering {
    inherit (config.home) homeDirectory;
    inherit (agentToken) configDir;
    settings = cfg.claude.composedSettings;
    secretPathPolicy = builtins.fromJSON (
      builtins.readFile (evalProfileSource + "/hooks/rules/secret_path_policy.json")
    );
  };

  claudeBaseSettings = builtins.fromJSON (
    builtins.readFile (evalProfileSource + "/claude/settings.json")
  );
  claudeSourceSettings = lib.recursiveUpdate claudeBaseSettings cfg.claude.settings;
  claudeSettings = pkgs.writeText "agent-harness-claude-settings.json" (
    builtins.toJSON cfg.claude.composedSettings
  );

  codexFormat = pkgs.formats.toml { };
  codexBaseSettings = builtins.fromTOML (
    builtins.readFile (evalProfileSource + "/codex/config.toml")
  );
  codexSourceSettings = lib.recursiveUpdate codexBaseSettings cfg.codex.settings;
  codexSettings = codexFormat.generate "agent-harness-codex-settings.toml" cfg.codex.composedSettings;

  composedSource = pkgs.runCommand "agent-harness-${cfg.profile}-source" { } ''
    cp -R ${lib.escapeShellArg "${profileSource}/."} "$out"
    chmod -R u+w "$out"
    install -m 0644 ${lib.escapeShellArg "${agentsMd}"} "$out/AGENTS.md"
    install -m 0644 ${lib.escapeShellArg "${commandPermissions}"} \
      "$out/command_permissions.json"
    install -m 0644 ${claudeSettings} "$out/claude/settings.json"
    install -m 0644 ${codexSettings} "$out/codex/config.toml"
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
      --dsh-profile ${lib.escapeShellArg cfg.dsh.profile} \
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
  imports = [ ./agent_github_app_token_module.nix ];

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
        carry `passthru.tree`, a path to its unbuilt tree: evaluation then reads settings,
        profiles, and the secret path policy from `tree` so it never builds the source. `tree`
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

      composedSettings = lib.mkOption {
        type = lib.types.attrsOf lib.types.anything;
        readOnly = true;
        internal = true;
        description = "Codex source settings after profile, override, and opt-in composition.";
      };
    };

    claude = {
      settings = lib.mkOption {
        type = lib.types.attrsOf lib.types.anything;
        default = { };
        description = "Claude Code settings recursively merged over the selected profile.";
      };

      composedSettings = lib.mkOption {
        type = lib.types.attrsOf lib.types.anything;
        readOnly = true;
        internal = true;
        description = "Claude Code source settings after profile, override, and opt-in composition.";
      };
    };

    github.agentToken = {
      enable = lib.mkEnableOption ''
        an agent-scoped GitHub token for gh inside the Claude Code sandbox, Codex shell
        commands, and git push. The token's own permissions are the safety boundary: anything
        running in the sandbox can use it'';

      configDir = lib.mkOption {
        type = lib.types.str;
        default = "${config.home.homeDirectory}/.config/agent-harness/gh";
        defaultText = lib.literalExpression ''"''${config.home.homeDirectory}/.config/agent-harness/gh"'';
        description = "Absolute GH_CONFIG_DIR that holds the agent token, readable inside the sandbox.";
      };
    };

  } providerOptions;

  config = lib.mkIf cfg.enable {
    programs.agent-harness.claude.composedSettings = lib.pipe claudeSourceSettings [
      (
        settings:
        if agentToken.enable then
          agentGithubToken.applyToClaudeSettings agentToken.configDir settings
        else
          settings
      )
      (
        settings:
        if agentToken.enable && agentToken.source == "app" then
          agentGithubToken.denyPrivateKeyRead agentToken.app.privateKeyPath settings
        else
          settings
      )
    ];

    programs.agent-harness.codex.composedSettings = lib.pipe codexSourceSettings [
      (
        settings:
        if agentToken.enable then
          agentGithubToken.applyToCodexSettings {
            inherit (agentToken) configDir;
            claudeSettings = cfg.claude.composedSettings;
          } settings
        else
          settings
      )
      (
        settings:
        if agentToken.enable && agentToken.source == "app" then
          agentGithubToken.denyCodexPrivateKeyRead agentToken.app.privateKeyPath settings
        else
          settings
      )
    ];

    assertions = lib.optionals agentToken.enable [
      {
        assertion = lib.hasPrefix "/" agentToken.configDir;
        message = "programs.agent-harness.github.agentToken.configDir must be an absolute path.";
      }
      {
        assertion = agentTokenDenials == [ ];
        message =
          "programs.agent-harness.github.agentToken.configDir is unreadable in the sandbox "
          + "because of Read denials: ${lib.concatStringsSep ", " agentTokenDenials}";
      }
    ];

    home = {
      packages = [ cfg.package ];

      file = lib.mkMerge providerFiles;

      activation = providerActivations;
    };
  };
}
