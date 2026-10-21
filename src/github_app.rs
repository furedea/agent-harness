mod api;
mod jwt;
mod timestamp;
mod token_files;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

pub(crate) use api::{GitHubAppApi, GitHubRestApi};
use jwt::AppPrivateKey;
use token_files::{
    HostsToken, InstallationSelection, MintedToken, RefreshStatus, TokenExpiry, TokenFiles,
    TokenIssuer,
};

/// Refresh mints a new token once the current one has less than this left.
const MINT_BELOW_REMAINING_SECS: u64 = 20 * 60;
/// The refresh job checks every 5 minutes and mints below 20 minutes left, so a healthy token keeps
/// at least 15 minutes.
const STALE_AFTER_REMAINING_SECS: u64 = 5 * 60;

/// Inputs for minting an installation access token into `config_dir`.
#[derive(Debug, Clone)]
pub(crate) struct RefreshRequest {
    pub(crate) app_id: u64,
    pub(crate) installation_id: Option<u64>,
    pub(crate) private_key: PathBuf,
    pub(crate) config_dir: PathBuf,
    /// Mints even when the current token still has enough time left.
    pub(crate) force: bool,
}

/// Mints a fresh installation token and writes it for gh, recording any failure beside it.
///
/// Skips minting while `hosts.yml` holds the recorded token, issued for the requested App and
/// installation, with at least 20 minutes left, unless forced or the last refresh failed.
pub(crate) fn refresh(
    request: &RefreshRequest,
    api: &impl GitHubAppApi,
    now_unix: u64,
) -> Result<()> {
    let files = TokenFiles::new(&request.config_dir);
    if !request.force && can_keep_current_token(&files, request, now_unix) {
        return Ok(());
    }
    match mint_token(request, api, now_unix) {
        Ok(minted) => files.write_token(&minted, now_unix),
        Err(error) => {
            files.record_failure(&format!("{error:#}"), now_unix)?;
            Err(error)
        }
    }
}

/// Describes the current token, failing when it is missing, expiring, not the token in `hosts.yml`,
/// or the last refresh failed.
pub(crate) fn status(config_dir: &Path, now_unix: u64) -> Result<String> {
    let files = TokenFiles::new(config_dir);
    let status = files.status()?.unwrap_or_default();
    let RefreshStatus {
        token,
        last_failure,
    } = status;
    let Some(token) = token else {
        bail!(
            "no GitHub App token has been minted in {}{}",
            config_dir.display(),
            last_failure.map_or_else(String::new, |failure| format!(": {}", failure.message)),
        );
    };
    if let Some(failure) = last_failure {
        bail!(
            "the last GitHub App token refresh failed: {}; the current token expires at {}",
            failure.message,
            token.expires_at,
        );
    }
    let hosts_path = files.hosts_path();
    match files.hosts_token(&token)? {
        HostsToken::Recorded => {}
        HostsToken::Missing => bail!(
            "{} is missing; the next refresh mints a new GitHub App token",
            hosts_path.display(),
        ),
        HostsToken::Different => bail!(
            "{} holds a different token than the last refresh recorded; the next refresh mints a \
             new GitHub App token",
            hosts_path.display(),
        ),
        HostsToken::Unverifiable => bail!(
            "the GitHub App token was recorded by an older release; the next refresh mints a new \
             one"
        ),
    }
    let remaining_secs = token.expires_at_unix.saturating_sub(now_unix);
    if remaining_secs <= STALE_AFTER_REMAINING_SECS {
        bail!(
            "the GitHub App token expires at {} ({} minutes left); the refresh job may have stopped",
            token.expires_at,
            remaining_secs / 60,
        );
    }
    Ok(format!(
        "GitHub App token valid until {} ({} minutes left)\n",
        token.expires_at,
        remaining_secs / 60,
    ))
}

/// An unreadable or corrupt status or `hosts.yml` counts as no token, so the caller mints a new one.
fn can_keep_current_token(files: &TokenFiles, request: &RefreshRequest, now_unix: u64) -> bool {
    let Ok(Some(RefreshStatus {
        token: Some(token),
        last_failure: None,
    })) = files.status()
    else {
        return false;
    };
    is_issued_for(&token, request)
        && matches!(files.hosts_token(&token), Ok(HostsToken::Recorded))
        && token.expires_at_unix.saturating_sub(now_unix) >= MINT_BELOW_REMAINING_SECS
}

/// Without a configured installation, a token minted for the App's only installation matches
/// because the App had only one then. Switching between a configured and an unset installation
/// never matches, so an unset one is resolved again.
fn is_issued_for(token: &TokenExpiry, request: &RefreshRequest) -> bool {
    let Some(issuer) = token.issuer else {
        return false;
    };
    let is_same_installation = match request.installation_id {
        Some(installation_id) => issuer.installation_id == installation_id,
        None => true,
    };
    let expected_selection = match request.installation_id {
        Some(_) => InstallationSelection::Configured,
        None => InstallationSelection::OnlyInstallation,
    };
    issuer.app_id == request.app_id
        && issuer.selection == expected_selection
        && is_same_installation
}

fn mint_token(
    request: &RefreshRequest,
    api: &impl GitHubAppApi,
    now_unix: u64,
) -> Result<MintedToken> {
    let key = AppPrivateKey::read(&request.private_key)?;
    let app_jwt = jwt::app_jwt(request.app_id, &key, now_unix)?;
    let (installation_id, selection) = match request.installation_id {
        Some(installation_id) => (installation_id, InstallationSelection::Configured),
        None => (
            single_installation(api, &app_jwt)?,
            InstallationSelection::OnlyInstallation,
        ),
    };
    let token = api
        .create_installation_token(&app_jwt, installation_id)
        .with_context(|| format!("failed to mint a token for installation {installation_id}"))?;
    let issuer = TokenIssuer {
        app_id: request.app_id,
        installation_id,
        selection,
    };
    Ok(MintedToken { token, issuer })
}

fn single_installation(api: &impl GitHubAppApi, app_jwt: &str) -> Result<u64> {
    match api.installation_ids(app_jwt)?.as_slice() {
        [installation_id] => Ok(*installation_id),
        installations => bail!(
            "the GitHub App is installed {} times; set the installation ID explicitly",
            installations.len()
        ),
    }
}

#[cfg(test)]
pub(crate) mod test_support;

#[cfg(test)]
mod tests;
