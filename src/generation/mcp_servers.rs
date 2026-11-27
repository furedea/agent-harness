#![forbid(clippy::disallowed_methods, clippy::disallowed_types)]
//! MCP servers declared once in the source and rendered into each provider's own declaration.
//!
//! The source file `mcp_servers.json` holds `{"servers": {<name>: <server>}}`, where a server is
//! either a command (`command`, optional `args` and `env`) or a remote endpoint (`url`). Values are
//! written as they are: a credential belongs in the server's own environment or a wrapper command,
//! never in this file.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use toml_edit::{Array, Item, Table};

/// The servers the source declares, parsed and validated, by name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct McpServers(BTreeMap<String, McpServer>);

/// How a provider reaches one server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum McpServer {
    /// A process the provider starts and talks to over standard input and output.
    Command(ServerCommand),
    /// A remote server reached over streamable HTTP.
    Url(String),
}

/// The process of a command server; the field names are those Claude Code reads.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct ServerCommand {
    command: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    args: Vec<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    env: BTreeMap<String, String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDocument {
    servers: BTreeMap<String, RawServer>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawServer {
    command: Option<String>,
    args: Option<Vec<String>>,
    env: Option<BTreeMap<String, String>>,
    url: Option<String>,
}

impl McpServers {
    /// Parses and validates the `mcp_servers.json` text read from `path`.
    ///
    /// # Errors
    ///
    /// Returns an error naming `path`, and the server when one is invalid.
    pub(crate) fn parse(path: &Path, text: &str) -> Result<Self> {
        let raw: RawDocument = serde_json::from_str(text)
            .with_context(|| format!("failed to parse MCP servers {}", path.display()))?;
        raw.servers
            .into_iter()
            .map(|(name, server)| server_named(name, server))
            .collect::<Result<_>>()
            .map(Self)
            .with_context(|| format!("failed to parse MCP servers {}", path.display()))
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The `mcpServers` map of a Claude Code plugin manifest.
    pub(crate) fn claude_plugin_servers(&self) -> Value {
        self.0
            .iter()
            .map(|(name, server)| (name.clone(), server.claude_entry()))
            .collect::<serde_json::Map<_, _>>()
            .into()
    }

    /// The `mcp_servers` table of a Codex config.
    pub(crate) fn codex_table(&self) -> Item {
        let mut table: Table = self
            .0
            .iter()
            .map(|(name, server)| (name.clone(), Item::Table(server.codex_table())))
            .collect();
        table.set_implicit(true);
        Item::Table(table)
    }
}

impl McpServer {
    fn claude_entry(&self) -> Value {
        match self {
            Self::Command(command) => json!(command),
            Self::Url(url) => json!({"type": "http", "url": url}),
        }
    }

    fn codex_table(&self) -> Table {
        match self {
            Self::Command(command) => codex_command_table(command),
            Self::Url(url) => std::iter::once(("url", toml_edit::value(url))).collect(),
        }
    }
}

fn codex_command_table(server: &ServerCommand) -> Table {
    let mut table = Table::new();
    table.insert("command", toml_edit::value(&server.command));
    if !server.args.is_empty() {
        table.insert(
            "args",
            toml_edit::value(server.args.iter().collect::<Array>()),
        );
    }
    if !server.env.is_empty() {
        table.insert("env", Item::Table(string_table(&server.env)));
    }
    table
}

fn string_table(entries: &BTreeMap<String, String>) -> Table {
    entries
        .iter()
        .map(|(key, value)| (key.clone(), toml_edit::value(value)))
        .collect()
}

fn server_named(name: String, raw: RawServer) -> Result<(String, McpServer)> {
    ensure!(
        is_server_name(&name),
        "failed to parse MCP server {name:?}: a name may contain only ASCII letters, digits, \
         hyphens, and underscores"
    );
    let server =
        server_from(raw).with_context(|| format!("failed to parse MCP server {name:?}"))?;
    Ok((name, server))
}

/// The names Claude Code and Codex both accept and that read the same in a tool name.
fn is_server_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
}

fn server_from(raw: RawServer) -> Result<McpServer> {
    match (raw.command, raw.url) {
        (Some(command), None) => Ok(McpServer::Command(ServerCommand {
            command,
            args: raw.args.unwrap_or_default(),
            env: raw.env.unwrap_or_default(),
        })),
        (None, Some(url)) => url_server(url, raw.args.is_none() && raw.env.is_none()),
        (Some(_), Some(_)) => bail!("failed to read the server: set command or url, not both"),
        (None, None) => bail!("failed to read the server: set command or url"),
    }
}

fn url_server(url: String, without_command_fields: bool) -> Result<McpServer> {
    ensure!(
        without_command_fields,
        "failed to read the url server: it takes no args or env"
    );
    ensure!(
        url.starts_with("https://") || url.starts_with("http://"),
        "failed to read the url server: {url:?} must start with https:// or http://"
    );
    Ok(McpServer::Url(url))
}

#[cfg(test)]
mod tests;
