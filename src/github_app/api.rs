use anyhow::{Context, Result};
use serde::Deserialize;
use serde::de::DeserializeOwned;

const API_BASE_URL: &str = "https://api.github.com";
const API_VERSION: &str = "2022-11-28";
const USER_AGENT: &str = concat!("agent-harness/", env!("CARGO_PKG_VERSION"));

/// GitHub REST endpoints that authenticate with an App JWT.
pub(crate) trait GitHubAppApi {
    fn installation_ids(&self, app_jwt: &str) -> Result<Vec<u64>>;

    fn create_installation_token(
        &self,
        app_jwt: &str,
        installation_id: u64,
    ) -> Result<InstallationToken>;
}

/// Installation access token returned by GitHub, valid for one hour.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct InstallationToken {
    pub(crate) token: String,
    pub(crate) expires_at: String,
}

#[derive(Deserialize)]
struct Installation {
    id: u64,
}

/// `GitHubAppApi` backed by HTTPS requests to api.github.com.
pub(crate) struct GitHubRestApi {
    agent: ureq::Agent,
}

impl GitHubRestApi {
    pub(crate) fn new() -> Self {
        Self {
            agent: ureq::Agent::new_with_defaults(),
        }
    }
}

impl GitHubAppApi for GitHubRestApi {
    fn installation_ids(&self, app_jwt: &str) -> Result<Vec<u64>> {
        let request = self.agent.get(format!("{API_BASE_URL}/app/installations"));
        let installations: Vec<Installation> = read_json(
            with_headers(request, app_jwt).call(),
            "list App installations",
        )?;
        Ok(installations
            .into_iter()
            .map(|installation| installation.id)
            .collect())
    }

    fn create_installation_token(
        &self,
        app_jwt: &str,
        installation_id: u64,
    ) -> Result<InstallationToken> {
        let request = self.agent.post(format!(
            "{API_BASE_URL}/app/installations/{installation_id}/access_tokens"
        ));
        read_json(
            with_headers(request, app_jwt).send_empty(),
            "create an installation access token",
        )
    }
}

fn with_headers<B>(request: ureq::RequestBuilder<B>, app_jwt: &str) -> ureq::RequestBuilder<B> {
    request
        .header("Accept", "application/vnd.github+json")
        .header("Authorization", format!("Bearer {app_jwt}"))
        .header("User-Agent", USER_AGENT)
        .header("X-GitHub-Api-Version", API_VERSION)
}

fn read_json<T: DeserializeOwned>(
    response: Result<ureq::http::Response<ureq::Body>, ureq::Error>,
    action: &str,
) -> Result<T> {
    let mut response = response.with_context(|| format!("failed to {action}"))?;
    let body = response
        .body_mut()
        .read_to_string()
        .with_context(|| format!("failed to read the response to {action}"))?;
    serde_json::from_str(&body).with_context(|| format!("unexpected response to {action}"))
}
