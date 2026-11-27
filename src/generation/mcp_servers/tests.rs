use serde_json::json;

use super::*;

fn parsed(document: &serde_json::Value) -> Result<McpServers> {
    McpServers::parse(Path::new("mcp_servers.json"), &document.to_string())
}

fn error_of(document: &serde_json::Value) -> String {
    parsed(document).map_or_else(|error| format!("{error:#}"), |_| String::new())
}

#[test]
fn a_command_server_and_a_url_server_are_both_accepted() -> Result<()> {
    let servers = parsed(&json!({"servers": {
        "github": {"command": "github-mcp-server", "args": ["stdio"], "env": {"GITHUB_TOOLSETS": "repos"}},
        "docs": {"url": "https://docs.example.com/mcp"},
    }}))?;

    assert_eq!(
        servers.claude_plugin_servers(),
        json!({
            "docs": {"type": "http", "url": "https://docs.example.com/mcp"},
            "github": {"command": "github-mcp-server", "args": ["stdio"], "env": {"GITHUB_TOOLSETS": "repos"}},
        })
    );
    Ok(())
}

#[test]
fn a_server_needs_exactly_one_of_command_and_url() {
    let both = error_of(&json!({"servers": {"x": {"command": "a", "url": "https://e.com"}}}));
    let neither = error_of(&json!({"servers": {"x": {"args": ["a"]}}}));

    assert!(both.contains("mcp_servers.json"), "{both}");
    assert!(both.contains("\"x\""), "{both}");
    assert!(neither.contains("\"x\""), "{neither}");
}

#[test]
fn a_url_server_takes_no_command_fields() {
    let message = error_of(&json!({"servers": {"x": {"url": "https://e.com", "args": ["a"]}}}));

    assert!(message.contains("\"x\""), "{message}");
}

#[test]
fn an_unknown_server_field_is_rejected_instead_of_dropped() {
    let message = error_of(&json!({"servers": {"x": {"command": "a", "headers": {"A": "b"}}}}));

    assert!(message.contains("headers"), "{message}");
}

#[test]
fn a_server_name_is_limited_to_letters_digits_hyphens_and_underscores() {
    let message = error_of(&json!({"servers": {"my server": {"command": "a"}}}));

    assert!(message.contains("my server"), "{message}");
}

#[test]
fn a_url_must_be_http_or_https() {
    let message = error_of(&json!({"servers": {"x": {"url": "file:///tmp/mcp"}}}));

    assert!(message.contains("file:///tmp/mcp"), "{message}");
}

#[test]
fn no_servers_render_nothing_for_either_provider() -> Result<()> {
    let servers = parsed(&json!({"servers": {}}))?;

    assert!(servers.is_empty());
    assert!(McpServers::default().is_empty());
    Ok(())
}

#[test]
fn codex_tables_carry_command_args_env_and_url() -> Result<()> {
    let servers = parsed(&json!({"servers": {
        "github": {"command": "github-mcp-server", "args": ["stdio"], "env": {"GITHUB_TOOLSETS": "repos"}},
        "docs": {"url": "https://docs.example.com/mcp"},
    }}))?;
    let mut document = toml_edit::DocumentMut::new();
    document.insert("mcp_servers", servers.codex_table());

    let expected = "[mcp_servers.docs]\nurl = \"https://docs.example.com/mcp\"\n\n\
        [mcp_servers.github]\ncommand = \"github-mcp-server\"\nargs = [\"stdio\"]\n\n\
        [mcp_servers.github.env]\nGITHUB_TOOLSETS = \"repos\"\n";
    assert_eq!(
        document
            .to_string()
            .parse::<toml_edit::DocumentMut>()?
            .to_string(),
        expected.parse::<toml_edit::DocumentMut>()?.to_string()
    );
    Ok(())
}
