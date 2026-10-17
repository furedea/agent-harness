# ADR-0022: Leave agent commits unsigned inside the Claude sandbox

- Status: Accepted
- Date: 2026-10-17

In the context of the opt-in agent GitHub token of ADR-0020, whose README advised excluding
`git add *`, `git status *`, `git commit *`, and `git push *` so that agent commits are signed outside
the sandbox, facing repository git hooks that then also run outside the sandbox, where an agent can
run unsandboxed code by editing a hook configuration such as `lefthook.yml`, a script a hook calls,
or build settings that a hook's tool reads, we decided for documenting a second, recommended setup
in which the Claude source settings' `env` sets `GIT_CONFIG_COUNT`, `GIT_CONFIG_KEY_n`, and
`GIT_CONFIG_VALUE_n` to `commit.gpgsign=false` and `tag.gpgsign=false` and
`sandbox.excludedCommands` lists no git command, so git, its hooks, and push run inside the sandbox
and push authenticates with the agent token, while keeping the signed setup documented and leaving
the composition of ADR-0020 unchanged, and against generating the git environment or removing git
exclusions in the module, which would add a second override of the source settings that ADR-0018
treats as authoritative, or keeping signing as the only documented setup, to keep code that hooks
run under the sandbox's filesystem and network restrictions and to keep the default branch signed
because agent changes reach it through squash merges that GitHub signs, accepting unsigned commits
on agent branches and in pull requests, a requirement for git 2.31 or later, and that a repository
which needs signed commits on every branch must use the signed setup and its hook risk.
