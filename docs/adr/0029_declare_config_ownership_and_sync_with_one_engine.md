# ADR-0029: Declare config ownership and sync with one engine

- Status: Accepted
- Date: 2026-11-12

In the context of six providers whose mutable configs the harness keeps in step with the rendered
ones, each by its own code (Claude `existing.extend(generated)`, Devin `starts_with("Exec(")`,
Codex `MANAGED_KEYS` plus a sidecar, Hermes a special case for `plugins`, Pi a key list, dsh a
bundle registration) and each tied to one format, facing a policy that lived only in code and a
`ConfigSync` enum that named providers' behaviors instead of the mechanism, we decided to declare
what the harness owns in the provider spec, to separate the format codec from the ownership policy,
and to apply any declaration with one engine, and against a `ConfigSync` variant per provider,
to make "what does the harness own in this config" a reviewable value, accepting that the
declaration language must grow when a provider owns a part it cannot express.

## Pattern

- **A declaration, not a kind.** `ConfigSync` is a struct in `provider/ownership.rs`: a
    `ConfigFormat` (`Json`, `Toml`, `Yaml`), an ordered list of `Ownership` claims, what to do when
    the installed config is missing (`WhenMissing::Create` or `Skip`), and the command lists whose
    leading `~/` becomes the home on a sync. Every name describes a mechanism; none names a provider.
- **Claims are the ownership.** Each claim addresses a part of the config by the keys leading to it:
    - `Keys { scope, when_absent }`: top-level keys, replaced as a whole. The scope is `Every`,
        `EveryExcept(..)`, or `Listed(..)`; an owned key the source no longer has is `Keep`t or
        `Remove`d.
    - `Entries { path, owned }`: the lists under a mapping, where the entries that match (`Exec(`
        prefix) are replaced and every other entry stays.
    - `ListUnion { path }`: a list that keeps what is installed and gains what it lacks.
    - `MappingEntries { path }`: the entries of a mapping, each replaced on its own.
    - `TrackedKeys { path, sidecar, formerly_managed }`: the entries a sidecar file next to the
        config says the previous sync wrote, so a key dropped from the source is removed and a key
        the user or the provider added never is.
- **The codec is separate.** `config_sync::codec` parses text into a `ConfigTree` and back; it
    answers path and value questions and knows nothing of ownership. JSON and YAML use their native
    values; TOML uses `toml_edit` so comments, layout, and inline tables the user wrote survive.
- **One engine.** `config_sync::Merge` takes the declaration, the generated document, and the
    `Installed` config, applies the claims in order, and returns the config text and, when a claim
    tracks keys, the sidecar text. `shell::sync` (the `sync-config` commands) and the renderer (which
    merges Codex and Pi configs into the output) both call it, so the install and the sync cannot
    drift.
- **Parsed once, at the boundary.** The loader reads the installed config and its sidecar into the
    snapshot. `Installed` parses them once, with the path in the error, and is what computation
    receives. A missing or unreadable sidecar is not an error: it only forgoes removals, apart from
    the `formerly_managed` keys. Computation under `config_sync` is `forbid`den to do I/O like the
    rest of the core ([ADR-0028](0028_return_file_operations_as_values.md)).
- **Render-time mechanisms are declared too.** `ConfigSpec::rendered` says how the rendered file is
    produced (`WithHooks`, `Fixed`, `SourceFile`, `SourceTomlWithProtection`), so the renderer
    matches on a mechanism, and `merges_into_installed` replaces a test of the provider's kind.

## The declarations

| Provider | Format | Claims                                                                                  |
| -------- | ------ | --------------------------------------------------------------------------------------- |
| Claude   | JSON   | `Keys(Every)`; `~/` of `sandbox.excludedCommands` is expanded on a sync                 |
| Devin    | JSON   | `Entries(permissions, "Exec(")`, then `Keys(EveryExcept(permissions))`                  |
| Codex    | TOML   | `Keys(Listed(..), Remove)`, then `TrackedKeys(shell_environment_policy.set, sidecar)`   |
| Hermes   | YAML   | `Keys(EveryExcept(plugins))`, then `ListUnion(plugins.enabled)`                         |
| Pi       | JSON   | `Keys(Listed(defaultProvider, defaultModel, defaultThinkingLevel), Keep)`               |
| dsh      | JSON   | `MappingEntries(dependencies)`, then `ListUnion(dsh.profile.bundles)`; `Skip` if absent |

Claude's `permissions` is replaced whole because the key is generated whole
([ADR-0016](0016_materialize_mutable_claude_settings.md)); Devin replaces only `Exec(...)` entries because the other entries of its permission lists are the
user's. The characterization tests in `shell/sync/tests` pin each row's observable result, and
the golden install is unchanged.

## Consequences

- Adding a provider config declares claims; it adds no merge code. A new shape of ownership is a
    new `Ownership` variant with one function in the engine and a test, not a per-provider module.
- `generation/{devin,hermes,pi}_config` and `codex_managed_state` are gone; the Hermes managed
    config and the Codex managed key list live in the registry, where the rest of the declaration is.
- The sidecar records each tracking claim under its path joined with `_`, which for Codex is the
    `shell_environment_policy_set` field the file always had, so existing sidecars still read.
- `ListUnion` and `MappingEntries` own only what they name. Hermes no longer replaces other keys
    that a generated `plugins` mapping might hold, and a Pi `sync-config` whose rendered file is
    missing is now an error instead of a no-op; neither can arise from the harness's own output.
- Claims must not overlap; `Keys(EveryExcept(..))` leaves out what another claim owns. A registry
    test rejects two claims whose paths contain one another, but not a key scope that overlaps a
    path claim.
- Roundtrips go through `serde_json` and `serde_norway` values for JSON and YAML, so only TOML keeps
    the user's formatting. That was already so and is unchanged.
- Wave 4 of the plan replaces the raw text the source snapshot holds with parsed types, as this
    change does for installed configs.
