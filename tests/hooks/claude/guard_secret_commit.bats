#!/usr/bin/env bats
# Tests for .claude/hooks/guard_secret_commit.sh

setup() {
  load test-helper/setup
  create_temp_git_repo
}

# Helper: run the hook from within the temp repo so it picks up staged files.
run_hook() {
  local cmd="${1:-git commit -m test}"
  run bash -c "cd '$TEMP_REPO' && bash '$HOOK_DIR/guard_secret_commit.sh'" <<<"$(make_input "$cmd")"
}

# --- Blocked cases: sensitive filename patterns ---

@test "blocks paths declared by the secret commit policy" {
  local _policy="$BATS_TEST_TMPDIR/secret_commit_policy.json"
  jq -n '{
    version: 1,
    rules: [{pattern: "(^|/)custom-sensitive\\.txt$", reason: "Test policy."}]
  }' >"$_policy"
  export AGENT_SECRET_COMMIT_POLICY="$_policy"
  stage_file "custom-sensitive.txt" "value"

  run_hook

  [ "$status" -eq 2 ]
  [[ "$output" == *"custom-sensitive.txt"* ]]
  [[ "$output" == *"Test policy."* ]]
}

@test "blocks commits when the secret commit policy is invalid" {
  local _policy="$BATS_TEST_TMPDIR/invalid_secret_commit_policy.json"
  jq -n '{version: 1, rules: []}' >"$_policy"
  export AGENT_SECRET_COMMIT_POLICY="$_policy"
  stage_file "README.md" "value"

  run_hook

  [ "$status" -eq 2 ]
  [[ "$output" == *"invalid secret commit policy"* ]]
}

@test "blocks .env file" {
  stage_file ".env" "SECRET=abc"
  run_hook
  [ "$status" -eq 2 ]
  [[ "$output" == *".env"* ]]
}

@test "blocks .env.local file" {
  stage_file ".env.local" "SECRET=abc"
  run_hook
  [ "$status" -eq 2 ]
  [[ "$output" == *".env.local"* ]]
}

@test "blocks .env.production file" {
  stage_file ".env.production" "SECRET=abc"
  run_hook
  [ "$status" -eq 2 ]
  [[ "$output" == *".env.production"* ]]
}

@test "blocks .env.development file" {
  stage_file ".env.development" "SECRET=abc"
  run_hook
  [ "$status" -eq 2 ]
}

@test "blocks credentials file" {
  stage_file "credentials" "key=value"
  run_hook
  [ "$status" -eq 2 ]
  [[ "$output" == *"credentials"* ]]
}

@test "blocks credential file (singular)" {
  stage_file "credential" "key=value"
  run_hook
  [ "$status" -eq 2 ]
}

@test "blocks secrets file" {
  stage_file "secrets" "data"
  run_hook
  [ "$status" -eq 2 ]
}

@test "blocks secret file (singular)" {
  stage_file "secret" "data"
  run_hook
  [ "$status" -eq 2 ]
}

@test "blocks .pem file" {
  stage_file "server.pem" "cert"
  run_hook
  [ "$status" -eq 2 ]
  [[ "$output" == *".pem"* ]]
}

@test "blocks .key file" {
  stage_file "private.key" "key"
  run_hook
  [ "$status" -eq 2 ]
}

@test "blocks .p12 file" {
  stage_file "cert.p12" "data"
  run_hook
  [ "$status" -eq 2 ]
}

@test "blocks .pkcs12 file" {
  stage_file "cert.pkcs12" "data"
  run_hook
  [ "$status" -eq 2 ]
}

@test "blocks .jks file" {
  stage_file "keystore.jks" "data"
  run_hook
  [ "$status" -eq 2 ]
}

@test "blocks .pfx file" {
  stage_file "cert.pfx" "data"
  run_hook
  [ "$status" -eq 2 ]
}

@test "blocks id_rsa file" {
  stage_file "id_rsa" "key"
  run_hook
  [ "$status" -eq 2 ]
}

@test "blocks id_ed25519 file" {
  stage_file "id_ed25519" "key"
  run_hook
  [ "$status" -eq 2 ]
}

@test "blocks .aws/ config" {
  stage_file ".aws/credentials" "data"
  run_hook
  [ "$status" -eq 2 ]
}

@test "blocks .gcp/ config" {
  stage_file ".gcp/service-account.json" "data"
  run_hook
  [ "$status" -eq 2 ]
}

@test "blocks application credential data files" {
  stage_file "config/client_secret.json" "data"
  run_hook
  [ "$status" -eq 2 ]
}

@test "blocks multiple sensitive files and lists all" {
  stage_file ".env" "a"
  stage_file "secrets" "b"
  run_hook
  [ "$status" -eq 2 ]
  [[ "$output" == *".env"* ]]
  [[ "$output" == *"secrets"* ]]
}

# --- Allowed cases ---

@test "allows safe files" {
  stage_file "README.md" "hello"
  run_hook
  [ "$status" -eq 0 ]
}

@test "allows source filenames containing secret terminology" {
  stage_file "src/secret.rs" "source"
  stage_file "src/secrets.rs" "source"
  stage_file "src/credentials.rs" "source"
  stage_file "src/secret_parser.rs" "source"
  run_hook
  [ "$status" -eq 0 ]
}

@test "allows tracked secret scanner hook scripts" {
  stage_file "agents/hooks/guard_secret_content.sh" "#!/bin/bash"
  stage_file "codex/hooks/adapt_guard_secret_content.sh" "#!/bin/bash"
  stage_file "tests/hooks/claude/guard_secret_content.bats" "#!/usr/bin/env bats"
  stage_file "tests/hooks/codex/adapt_guard_secret_content.bats" "#!/usr/bin/env bats"
  run_hook
  [ "$status" -eq 0 ]
}

@test "allows tracked secret policy implementation" {
  stage_file "src/generation/secret_path_policy.rs" "pub(crate) fn read_policy() {}"
  run_hook
  [ "$status" -eq 0 ]
}

@test "allows when no files are staged" {
  # No files staged beyond initial commit
  run_hook
  [ "$status" -eq 0 ]
}

@test "passes through non-commit commands" {
  stage_file ".env" "SECRET=abc"
  run_hook "git status"
  [ "$status" -eq 0 ]
}

@test "passes through non-git commands" {
  run_hook "echo hello"
  [ "$status" -eq 0 ]
}
