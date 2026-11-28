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

pub(crate) use crate::provider::spec::McpEntry;

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

    /// The servers by name, each in the form `entry` gives a url server.
    pub(crate) fn json_map(&self, entry: McpEntry) -> Value {
        self.0
            .iter()
            .map(|(name, server)| (name.clone(), server.json_entry(entry)))
            .collect::<serde_json::Map<_, _>>()
            .into()
    }

    /// The servers as a TOML table without a header of its own, each in the form `entry` gives a
    /// url server.
    ///
    /// # Errors
    ///
    /// Returns an error when a server holds a value TOML cannot express, which a validated server
    /// never does.
    pub(crate) fn toml_table(&self, entry: McpEntry) -> Result<Item> {
        let mut table = self
            .0
            .iter()
            .map(|(name, server)| server.toml_item(name, entry))
            .collect::<Result<Table>>()?;
        table.set_implicit(true);
        Ok(Item::Table(table))
    }
}

impl McpServer {
    fn json_entry(&self, entry: McpEntry) -> Value {
        match self {
            Self::Command(command) => json!(command),
            Self::Url(url) => url_entry(url, entry),
        }
    }

    fn toml_item(&self, name: &str, entry: McpEntry) -> Result<(String, Item)> {
        let table = match self {
            Self::Command(command) => Ok(command_table(command)),
            Self::Url(_) => flat_table(&self.json_entry(entry)),
        };
        table
            .map(|table| (name.to_owned(), Item::Table(table)))
            .with_context(|| format!("failed to render MCP server {name:?} as TOML"))
    }
}

fn url_entry(url: &str, entry: McpEntry) -> Value {
    match entry {
        McpEntry::TypedUrl => json!({"type": "http", "url": url}),
        McpEntry::Url => json!({"url": url}),
        McpEntry::UrlWithTransport => json!({"url": url, "transport": "http"}),
        McpEntry::ServerUrl => json!({"serverUrl": url}),
    }
}

fn command_table(server: &ServerCommand) -> Table {
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

/// A JSON object of strings as a TOML table.
fn flat_table(value: &Value) -> Result<Table> {
    let Value::Object(object) = value else {
        bail!("failed to convert {value} to a TOML table: it is not an object");
    };
    object
        .iter()
        .map(|(key, value)| {
            let text = value
                .as_str()
                .with_context(|| format!("failed to convert {key} to TOML: not a string"))?;
            Ok((key.clone(), toml_edit::value(text)))
        })
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
