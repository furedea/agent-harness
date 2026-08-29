{ agentHarness, pkgs }:
let
  inherit (pkgs) lib;

  commandLine = command: lib.escapeShellArgs (map toString command);

  installer = command: {
    executable = toString (builtins.head command);
    arguments = map toString (builtins.tail command);
  };
in
{
  buildSkillFromCommand =
    {
      name,
      command,
    }:
    assert lib.assertMsg (command != [ ]) "buildSkillFromCommand requires a non-empty command";
    pkgs.runCommand "${name}-agent-skill" { } ''
      mkdir -p "$out"
      ${commandLine command} > "$out/SKILL.md"
      if [ ! -s "$out/SKILL.md" ]; then
        echo "skill command produced an empty SKILL.md" >&2
        exit 1
      fi
    '';

  buildHookBundleFromCommands =
    {
      name,
      commands,
      commandReplacements ? [ ],
    }:
    assert lib.assertMsg (commands != [ ]) "buildHookBundleFromCommands requires commands";
    assert lib.assertMsg (builtins.all (
      command: command != [ ]
    ) commands) "buildHookBundleFromCommands commands must not be empty";
    let
      spec = pkgs.writeText "${name}-hook-bundle-spec.json" (
        builtins.toJSON {
          version = 1;
          installers = map installer commands;
          command_replacements = commandReplacements;
        }
      );
    in
    pkgs.runCommand "${name}-agent-hook-bundle" { } ''
      ${lib.getExe agentHarness} generate-hook-bundle \
        --spec ${spec} \
        --output "$out"
    '';
}
