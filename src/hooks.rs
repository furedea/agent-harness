use std::path::Path;

use anyhow::{Context, Result};
use serde_json::{Value, json};

/// Write Claude Code hook settings as JSON.
///
/// # Errors
///
/// Returns an error when the output directory cannot be created or the file
/// cannot be written.
pub fn write_claude_hooks(path: &Path) -> Result<()> {
    write_json(path, &claude_hooks())
}

/// Write Codex hook settings as JSON.
///
/// # Errors
///
/// Returns an error when the output directory cannot be created or the file
/// cannot be written.
pub fn write_codex_hooks(path: &Path) -> Result<()> {
    write_json(path, &codex_hooks())
}

pub fn claude_hooks() -> Value {
    json!({
        "UserPromptSubmit": [
            {
                "matcher": "",
                "hooks": [claude_hook("$HOME/.claude/hooks/guard_secret_content.sh prompt")]
            }
        ],
        "PreToolUse": [
            {
                "matcher": "Bash",
                "hooks": [
                    claude_hook("$HOME/.claude/hooks/audit_tool_call.sh"),
                    claude_hook("$HOME/.claude/hooks/guard_forbidden_commands.sh"),
                    claude_hook("$HOME/.claude/hooks/guard_secret_commit.sh"),
                    claude_hook("$HOME/.claude/hooks/guard_dangerous_git.sh"),
                    claude_hook("$HOME/.claude/hooks/guard_allowed_commands.sh")
                ]
            },
            {
                "matcher": "Read",
                "hooks": [
                    claude_hook("$HOME/.claude/hooks/audit_tool_call.sh"),
                    claude_hook("$HOME/.claude/hooks/guard_secret_content.sh read")
                ]
            },
            {
                "matcher": "Write|Edit|MultiEdit",
                "hooks": [
                    claude_hook("$HOME/.claude/hooks/audit_tool_call.sh"),
                    claude_hook("$HOME/.claude/hooks/guard_harness_files.sh"),
                    claude_hook("$HOME/.claude/hooks/guard_secret_content.sh write")
                ]
            }
        ],
        "PostToolUse": [
            {
                "matcher": "Write|Edit|MultiEdit",
                "hooks": [
                    claude_hook_if("$HOME/.claude/hooks/lint_format_py.sh", "Write(*.py)|Edit(*.py)|MultiEdit(*.py)"),
                    claude_hook_if("$HOME/.claude/hooks/lint_format_sh.sh", "Write(*.sh)|Edit(*.sh)|MultiEdit(*.sh)"),
                    claude_hook_if("$HOME/.claude/hooks/lint_format_js.sh", "Write(*.js)|Edit(*.js)|MultiEdit(*.js)|Write(*.ts)|Edit(*.ts)|MultiEdit(*.ts)|Write(*.jsx)|Edit(*.jsx)|MultiEdit(*.jsx)|Write(*.tsx)|Edit(*.tsx)|MultiEdit(*.tsx)"),
                    claude_hook_if("$HOME/.claude/hooks/lint_format_rs.sh", "Write(*.rs)|Edit(*.rs)|MultiEdit(*.rs)"),
                    claude_hook_if("$HOME/.claude/hooks/lint_format_nix.sh", "Write(*.nix)|Edit(*.nix)|MultiEdit(*.nix)"),
                    claude_hook_if("$HOME/.claude/hooks/lint_format_md.sh", "Write(*.md)|Edit(*.md)|MultiEdit(*.md)|Write(*.markdown)|Edit(*.markdown)|MultiEdit(*.markdown)"),
                    claude_hook_if("$HOME/.claude/hooks/lint_format_json_toml.sh", "Write(*.json)|Edit(*.json)|MultiEdit(*.json)|Write(*.toml)|Edit(*.toml)|MultiEdit(*.toml)"),
                    claude_hook_if("$HOME/.claude/hooks/lint_format_gha.sh", "Write(*.yml)|Edit(*.yml)|MultiEdit(*.yml)|Write(*.yaml)|Edit(*.yaml)|MultiEdit(*.yaml)"),
                    claude_hook_if("$HOME/.claude/hooks/lint_format_txt.sh", "Write(*.txt)|Edit(*.txt)|MultiEdit(*.txt)"),
                    claude_hook_if("$HOME/.claude/hooks/lint_format_lua.sh", "Write(*.lua)|Edit(*.lua)|MultiEdit(*.lua)"),
                    claude_hook_if("$HOME/.claude/hooks/lint_format_tex.sh", "Write(*.tex)|Edit(*.tex)|MultiEdit(*.tex)|Write(*.bib)|Edit(*.bib)|MultiEdit(*.bib)|Write(*.cls)|Edit(*.cls)|MultiEdit(*.cls)|Write(*.sty)|Edit(*.sty)|MultiEdit(*.sty)")
                ]
            },
            {
                "matcher": "Bash|Edit|MultiEdit|Write|WebFetch|WebSearch|Task|Agent",
                "hooks": [claude_hook("$HOME/.claude/hooks/audit_tool_call.sh")]
            }
        ],
        "Stop": [
            {
                "matcher": "",
                "hooks": [
                    claude_hook("$HOME/.claude/hooks/run_related_tests.sh"),
                    claude_hook("$HOME/.claude/hooks/notify_macos_done.sh")
                ]
            }
        ],
        "SubagentStop": [
            {
                "matcher": "",
                "hooks": [claude_hook("$HOME/.claude/hooks/notify_macos_done.sh")]
            }
        ],
        "Notification": [
            {
                "matcher": "",
                "hooks": [claude_hook("$HOME/.claude/hooks/notify_macos_await.sh")]
            }
        ],
        "PermissionDenied": [
            {
                "hooks": [claude_hook("$HOME/.claude/hooks/audit_permission_denied.sh")]
            }
        ],
        "PreCompact": [
            {
                "matcher": "",
                "hooks": [claude_hook("$HOME/.claude/hooks/audit_compaction.sh")]
            }
        ],
        "SessionStart": [
            {
                "matcher": "compact|resume",
                "hooks": [claude_hook("$HOME/.claude/hooks/audit_compaction.sh")]
            }
        ]
    })
}

pub fn codex_hooks() -> Value {
    let shell_matcher = "^(Bash|exec_command|functions\\.exec_command)$";
    let edit_matcher = "^apply_patch$|^Edit$|^Write$";

    json!({
        "hooks": {
            "UserPromptSubmit": [
                {
                    "hooks": [
                        codex_hook(
                            "$HOME/.codex/hooks/adapt_guard_secret_content.sh prompt",
                            "Scanning prompt for sensitive information",
                            30
                        )
                    ]
                }
            ],
            "PreToolUse": [
                {
                    "matcher": shell_matcher,
                    "hooks": [
                        codex_hook(
                            "$HOME/.codex/hooks/adapt_shell_command.sh $HOME/.claude/hooks/guard_forbidden_commands.sh",
                            "Checking forbidden command prefixes",
                            30
                        ),
                        codex_hook(
                            "$HOME/.codex/hooks/adapt_shell_command.sh $HOME/.claude/hooks/guard_secret_commit.sh",
                            "Checking staged files for secrets",
                            30
                        ),
                        codex_hook(
                            "$HOME/.codex/hooks/adapt_shell_command.sh $HOME/.claude/hooks/guard_dangerous_git.sh",
                            "Checking for dangerous git operations",
                            30
                        ),
                        codex_hook(
                            "$HOME/.codex/hooks/adapt_shell_command.sh $HOME/.claude/hooks/guard_allowed_commands.sh",
                            "Checking command policy",
                            30
                        )
                    ]
                },
                {
                    "matcher": edit_matcher,
                    "hooks": [
                        codex_hook(
                            "$HOME/.codex/hooks/adapt_harness_files.sh",
                            "Checking harness boundaries",
                            30
                        ),
                        codex_hook(
                            "$HOME/.codex/hooks/adapt_guard_secret_content.sh apply-patch",
                            "Scanning patch for sensitive information",
                            30
                        )
                    ]
                }
            ],
            "PostToolUse": [
                {
                    "matcher": edit_matcher,
                    "hooks": [
                        codex_hook(
                            "$HOME/.codex/hooks/adapt_lint_format.sh",
                            "Running lint/format hooks",
                            120
                        )
                    ]
                },
                {
                    "matcher": ".",
                    "hooks": [
                        codex_hook(
                            "$HOME/.claude/hooks/audit_tool_call.sh",
                            "Logging tool call",
                            10
                        )
                    ]
                }
            ]
        }
    })
}

fn claude_hook(command: &str) -> Value {
    json!({
        "command": command,
        "type": "command"
    })
}

fn claude_hook_if(command: &str, condition: &str) -> Value {
    json!({
        "command": command,
        "type": "command",
        "if": condition
    })
}

fn codex_hook(command: &str, status_message: &str, timeout: u64) -> Value {
    json!({
        "command": command,
        "statusMessage": status_message,
        "timeout": timeout,
        "type": "command"
    })
}

fn write_json(path: &Path, value: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory {}", parent.display()))?;
    }

    let content = serde_json::to_string_pretty(value)? + "\n";
    std::fs::write(path, content).with_context(|| format!("failed to write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_hooks_include_shell_and_lint_guards() {
        let hooks = claude_hooks();

        assert_eq!(
            hooks["PreToolUse"][0]["hooks"][1]["command"].as_str(),
            Some("$HOME/.claude/hooks/guard_forbidden_commands.sh"),
        );
        assert_eq!(
            hooks["PostToolUse"][0]["hooks"][3]["if"].as_str(),
            Some("Write(*.rs)|Edit(*.rs)|MultiEdit(*.rs)"),
        );
    }

    #[test]
    fn codex_hooks_include_adapters_for_shell_and_patch_tools() {
        let hooks = codex_hooks();

        assert_eq!(
            hooks["hooks"]["PreToolUse"][0]["matcher"].as_str(),
            Some("^(Bash|exec_command|functions\\.exec_command)$"),
        );
        assert_eq!(
            hooks["hooks"]["PreToolUse"][1]["hooks"][0]["command"].as_str(),
            Some("$HOME/.codex/hooks/adapt_harness_files.sh"),
        );
    }
}
