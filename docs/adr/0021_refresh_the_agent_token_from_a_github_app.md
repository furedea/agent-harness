# ADR-0021: Refresh the agent token from a GitHub App

- Status: Accepted
- Date: 2026-10-17

In the context of the opt-in agent GitHub token of ADR-0020, facing fine-grained personal access
tokens that expire and must be recreated by hand, we decided for an opt-in
`github.agentToken.source = "app"` beside the default `"pat"`, in which a per-user launchd agent or
systemd user timer runs a new `agent-harness github-app-token refresh` subcommand at login and every
50 minutes to sign an RS256 App JWT, mint a one-hour installation access token, and write it to
`hosts.yml` in `configDir` atomically with mode 0600, in which the module adds the App private key to
the generated `Read(...)` denials and `sandbox.filesystem.denyRead` and rejects a relative key path,
and in which each refresh outcome is recorded beside the token for a `github-app-token status`
subcommand that fails when the token is missing, nearly expired, or the last refresh failed, and
against shelling out to the openssl and curl CLIs, adding a JWT library, running a long-lived
daemon, or failing evaluation instead of adding the key denial, to let agents keep a valid token
without manual renewal while sandboxed code can never read the long-lived key, accepting three
direct dependencies (`ring` for signing, `base64`, and `ureq` with rustls for one HTTPS request)
whose licenses add ISC, BSD-3-Clause, and CDLA-Permissive-2.0 to the allow list, a private key that
is as sensitive as an SSH key, pushes and pull requests attributed to the App's bot, a token that
can approve pull requests and so must not count toward required approvals, support for github.com
only, and a status check that the user runs instead of a push notification.

## Amendment (2026-10-20)

Refreshing every 50 minutes left only a 10-minute margin, so a launchd deferral or sleep let the
one-hour token lapse (Issue 163). The job now checks the token every 5 minutes and mints a new one
only when less than 20 minutes remain, when `hosts.yml` is missing or does not hold the recorded
token, or when the App or installation changed; `github-app-token refresh --force` always mints.
