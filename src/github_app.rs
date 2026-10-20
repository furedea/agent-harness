mod api;
mod jwt;
mod timestamp;
mod token_files;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use api::InstallationToken;
pub(crate) use api::{GitHubAppApi, GitHubRestApi};
use jwt::AppPrivateKey;
use token_files::{HostsToken, RefreshStatus, TokenExpiry, TokenFiles, TokenIssuer};

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
        Ok((issuer, token)) => files.write_token(&token, issuer, now_unix),
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

/// Without a configured installation, any installation of the same App matches because the App
/// had only one when the token was minted.
fn is_issued_for(token: &TokenExpiry, request: &RefreshRequest) -> bool {
    token.issuer.is_some_and(|issuer| {
        issuer.app_id == request.app_id
            && request
                .installation_id
                .is_none_or(|installation_id| installation_id == issuer.installation_id)
    })
}

fn mint_token(
    request: &RefreshRequest,
    api: &impl GitHubAppApi,
    now_unix: u64,
) -> Result<(TokenIssuer, InstallationToken)> {
    let key = AppPrivateKey::read(&request.private_key)?;
    let app_jwt = jwt::app_jwt(request.app_id, &key, now_unix)?;
    let installation_id = match request.installation_id {
        Some(installation_id) => installation_id,
        None => single_installation(api, &app_jwt)?,
    };
    let token = api
        .create_installation_token(&app_jwt, installation_id)
        .with_context(|| format!("failed to mint a token for installation {installation_id}"))?;
    let issuer = TokenIssuer {
        app_id: request.app_id,
        installation_id,
    };
    Ok((issuer, token))
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
pub(crate) mod test_support {
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::sync::atomic::{AtomicU64, Ordering};

    use anyhow::{Context, Result, bail};

    pub(crate) enum TestKeyFormat {
        Pkcs1,
        Pkcs8,
    }

    /// Generates a throwaway RSA key with the openssl CLI so no key material is committed.
    pub(crate) fn generate_private_key(format: TestKeyFormat) -> Result<String> {
        let dir = TestDir::new("key")?;
        let pkcs8 = dir.path().join("pkcs8.pem");
        run_openssl(&[
            "genpkey",
            "-algorithm",
            "RSA",
            "-pkeyopt",
            "rsa_keygen_bits:2048",
            "-out",
            path_arg(&pkcs8)?,
        ])?;
        let key = match format {
            TestKeyFormat::Pkcs8 => pkcs8,
            TestKeyFormat::Pkcs1 => {
                let pkcs1 = dir.path().join("pkcs1.pem");
                run_openssl(&[
                    "pkey",
                    "-in",
                    path_arg(&pkcs8)?,
                    "-traditional",
                    "-out",
                    path_arg(&pkcs1)?,
                ])?;
                pkcs1
            }
        };
        Ok(std::fs::read_to_string(key)?)
    }

    fn run_openssl(args: &[&str]) -> Result<()> {
        let output = Command::new("openssl")
            .args(args)
            .output()
            .context("the openssl CLI is required to generate test keys")?;
        if !output.status.success() {
            bail!(
                "openssl {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        Ok(())
    }

    fn path_arg(path: &Path) -> Result<&str> {
        path.to_str().context("test path must be UTF-8")
    }

    pub(crate) struct TestDir(PathBuf);

    impl TestDir {
        pub(crate) fn new(name: &str) -> Result<Self> {
            static NEXT_ID: AtomicU64 = AtomicU64::new(0);
            let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "agent-harness-github-app-{name}-{}-{id}",
                std::process::id()
            ));
            std::fs::create_dir_all(&path)?;
            Ok(Self(path))
        }

        pub(crate) fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use anyhow::anyhow;

    use super::api::InstallationToken;
    use super::test_support::{TestDir, TestKeyFormat, generate_private_key};
    use super::*;

    const NOW: u64 = 1_792_067_696; // 2026-10-15T12:34:56Z
    const EXPIRES_AT: &str = "2026-10-15T13:34:56Z";

    #[derive(Default)]
    struct FakeApi {
        installations: Vec<u64>,
        fail_with: Option<&'static str>,
        token_requests: RefCell<Vec<(String, u64)>>,
    }

    impl GitHubAppApi for FakeApi {
        fn installation_ids(&self, _app_jwt: &str) -> Result<Vec<u64>> {
            Ok(self.installations.clone())
        }

        fn create_installation_token(
            &self,
            app_jwt: &str,
            installation_id: u64,
        ) -> Result<InstallationToken> {
            if let Some(message) = self.fail_with {
                return Err(anyhow!(message));
            }
            self.token_requests
                .borrow_mut()
                .push((app_jwt.to_owned(), installation_id));
            Ok(InstallationToken {
                token: format!("ghs_for_{installation_id}"),
                expires_at: EXPIRES_AT.to_owned(),
            })
        }
    }

    #[test]
    fn refresh_mints_a_token_for_the_configured_installation() -> Result<()> {
        let fixture = Fixture::new("configured")?;
        let api = FakeApi::default();

        refresh(&fixture.request(Some(42)), &api, NOW)?;

        let requests = api.token_requests.borrow();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].1, 42);
        assert_eq!(requests[0].0.split('.').count(), 3, "expected a signed JWT");
        assert!(fixture.hosts()?.contains("ghs_for_42"));
        Ok(())
    }

    #[test]
    fn refresh_resolves_the_single_installation() -> Result<()> {
        let fixture = Fixture::new("single")?;
        let api = FakeApi {
            installations: vec![7],
            ..FakeApi::default()
        };

        refresh(&fixture.request(None), &api, NOW)?;

        assert!(fixture.hosts()?.contains("ghs_for_7"));
        Ok(())
    }

    #[test]
    fn refresh_requires_an_installation_id_for_several_installations() -> Result<()> {
        let fixture = Fixture::new("several")?;
        let api = FakeApi {
            installations: vec![7, 8],
            ..FakeApi::default()
        };

        let error = refresh(&fixture.request(None), &api, NOW).unwrap_err();

        assert!(format!("{error:#}").contains("installed 2 times"));
        assert!(api.token_requests.borrow().is_empty());
        Ok(())
    }

    #[test]
    fn refresh_records_a_failure_and_reports_it_in_status() -> Result<()> {
        let fixture = Fixture::new("failure")?;
        refresh(&fixture.request(Some(42)), &FakeApi::default(), NOW)?;
        let failing = FakeApi {
            fail_with: Some("GitHub is unreachable"),
            ..FakeApi::default()
        };

        assert!(refresh(&fixture.request(Some(42)), &failing, NOW + 3_000).is_err());

        let error = status(&fixture.config_dir, NOW + 3_000).unwrap_err();
        assert!(format!("{error:#}").contains("GitHub is unreachable"));
        assert!(fixture.hosts()?.contains("ghs_for_42"));
        Ok(())
    }

    #[test]
    fn refresh_skips_minting_while_enough_time_remains() -> Result<()> {
        let fixture = Fixture::new("skip-fresh")?;
        refresh(&fixture.request(Some(42)), &FakeApi::default(), NOW)?;
        let api = FakeApi::default();

        refresh(&fixture.request(Some(42)), &api, NOW + 2_400)?;

        assert!(api.token_requests.borrow().is_empty());
        Ok(())
    }

    #[test]
    fn refresh_mints_when_the_token_is_near_expiry() -> Result<()> {
        let fixture = Fixture::new("near-expiry")?;
        refresh(&fixture.request(Some(42)), &FakeApi::default(), NOW)?;
        let api = FakeApi::default();

        refresh(&fixture.request(Some(42)), &api, NOW + 2_401)?;

        assert_eq!(api.token_requests.borrow().len(), 1);
        Ok(())
    }

    #[test]
    fn refresh_mints_when_no_token_was_recorded() -> Result<()> {
        let fixture = Fixture::new("no-record")?;
        let api = FakeApi::default();

        refresh(&fixture.request(Some(42)), &api, NOW)?;

        assert_eq!(api.token_requests.borrow().len(), 1);
        Ok(())
    }

    #[test]
    fn refresh_mints_when_the_recorded_status_is_corrupt() -> Result<()> {
        let fixture = Fixture::new("corrupt")?;
        std::fs::create_dir_all(&fixture.config_dir)?;
        std::fs::write(fixture.config_dir.join("agent_harness_token.json"), "{")?;
        let api = FakeApi::default();

        refresh(&fixture.request(Some(42)), &api, NOW)?;

        assert_eq!(api.token_requests.borrow().len(), 1);
        assert!(status(&fixture.config_dir, NOW)?.contains(EXPIRES_AT));
        Ok(())
    }

    #[test]
    fn refresh_mints_after_a_failed_refresh() -> Result<()> {
        let fixture = Fixture::new("retry")?;
        refresh(&fixture.request(Some(42)), &FakeApi::default(), NOW)?;
        let failing = FakeApi {
            fail_with: Some("GitHub is unreachable"),
            ..FakeApi::default()
        };
        assert!(refresh(&fixture.forced_request(), &failing, NOW).is_err());
        let api = FakeApi::default();

        refresh(&fixture.request(Some(42)), &api, NOW + 600)?;

        assert_eq!(api.token_requests.borrow().len(), 1);
        assert!(status(&fixture.config_dir, NOW + 600).is_ok());
        Ok(())
    }

    #[test]
    fn refresh_mints_when_hosts_yml_is_missing() -> Result<()> {
        let fixture = Fixture::new("hosts-missing")?;
        refresh(&fixture.request(Some(42)), &FakeApi::default(), NOW)?;
        std::fs::remove_file(fixture.config_dir.join("hosts.yml"))?;

        let error = status(&fixture.config_dir, NOW + 600).unwrap_err();
        assert!(format!("{error:#}").contains("hosts.yml is missing"));
        fixture.assert_refresh_mints(&fixture.request(Some(42)))
    }

    #[test]
    fn refresh_mints_when_hosts_yml_is_corrupt() -> Result<()> {
        let fixture = Fixture::new("hosts-corrupt")?;
        refresh(&fixture.request(Some(42)), &FakeApi::default(), NOW)?;
        std::fs::write(fixture.config_dir.join("hosts.yml"), "github.com: [")?;

        let error = status(&fixture.config_dir, NOW + 600).unwrap_err();
        assert!(format!("{error:#}").contains("hosts.yml"));
        fixture.assert_refresh_mints(&fixture.request(Some(42)))
    }

    #[test]
    fn refresh_mints_when_hosts_yml_holds_a_different_token() -> Result<()> {
        let fixture = Fixture::new("hosts-mismatch")?;
        refresh(&fixture.request(Some(42)), &FakeApi::default(), NOW)?;
        std::fs::write(
            fixture.config_dir.join("hosts.yml"),
            "github.com:\n    oauth_token: ghs_someone_else\n    user: x-access-token\n",
        )?;

        let error = status(&fixture.config_dir, NOW + 600).unwrap_err();
        assert!(format!("{error:#}").contains("different token"));
        fixture.assert_refresh_mints(&fixture.request(Some(42)))
    }

    #[test]
    fn refresh_mints_when_the_app_changes() -> Result<()> {
        let fixture = Fixture::new("app-switch")?;
        refresh(&fixture.request(Some(42)), &FakeApi::default(), NOW)?;

        fixture.assert_refresh_mints(&RefreshRequest {
            app_id: 99_999,
            ..fixture.request(Some(42))
        })
    }

    #[test]
    fn refresh_mints_when_the_installation_changes() -> Result<()> {
        let fixture = Fixture::new("installation-switch")?;
        refresh(&fixture.request(Some(42)), &FakeApi::default(), NOW)?;

        fixture.assert_refresh_mints(&fixture.request(Some(43)))
    }

    #[test]
    fn refresh_keeps_the_token_of_the_only_installation() -> Result<()> {
        let fixture = Fixture::new("only-installation")?;
        let single = FakeApi {
            installations: vec![42],
            ..FakeApi::default()
        };
        refresh(&fixture.request(None), &single, NOW)?;
        let api = FakeApi::default();

        refresh(&fixture.request(None), &api, NOW + 600)?;
        refresh(&fixture.request(Some(42)), &api, NOW + 600)?;

        assert!(api.token_requests.borrow().is_empty());
        Ok(())
    }

    #[test]
    fn refresh_mints_once_over_a_record_without_issuer_or_fingerprint() -> Result<()> {
        let fixture = Fixture::new("legacy")?;
        refresh(&fixture.request(Some(42)), &FakeApi::default(), NOW)?;
        std::fs::write(
            fixture.config_dir.join("agent_harness_token.json"),
            format!(
                r#"{{"token":{{"refreshed_at_unix":{NOW},"expires_at":"{EXPIRES_AT}","expires_at_unix":{}}},"last_failure":null}}"#,
                NOW + 3_600,
            ),
        )?;

        let error = status(&fixture.config_dir, NOW + 600).unwrap_err();
        assert!(format!("{error:#}").contains("older release"));
        fixture.assert_refresh_mints(&fixture.request(Some(42)))?;
        let api = FakeApi::default();
        refresh(&fixture.request(Some(42)), &api, NOW + 600)?;
        assert!(api.token_requests.borrow().is_empty());
        Ok(())
    }

    #[test]
    fn forced_refresh_mints_while_enough_time_remains() -> Result<()> {
        let fixture = Fixture::new("forced")?;
        refresh(&fixture.request(Some(42)), &FakeApi::default(), NOW)?;
        let api = FakeApi::default();

        refresh(&fixture.forced_request(), &api, NOW + 600)?;

        assert_eq!(api.token_requests.borrow().len(), 1);
        Ok(())
    }

    #[test]
    fn status_reports_a_fresh_token() -> Result<()> {
        let fixture = Fixture::new("fresh")?;
        refresh(&fixture.request(Some(42)), &FakeApi::default(), NOW)?;

        let report = status(&fixture.config_dir, NOW + 600)?;

        assert!(report.contains(EXPIRES_AT));
        assert!(report.contains("50 minutes left"));
        Ok(())
    }

    #[test]
    fn status_fails_when_the_token_is_about_to_expire() -> Result<()> {
        let fixture = Fixture::new("expiring")?;
        refresh(&fixture.request(Some(42)), &FakeApi::default(), NOW)?;

        let error = status(&fixture.config_dir, NOW + 3_400).unwrap_err();

        assert!(format!("{error:#}").contains("refresh job may have stopped"));
        Ok(())
    }

    #[test]
    fn status_fails_without_a_token() -> Result<()> {
        let fixture = Fixture::new("missing")?;

        let error = status(&fixture.config_dir, NOW).unwrap_err();

        assert!(format!("{error:#}").contains("no GitHub App token"));
        Ok(())
    }

    struct Fixture {
        _dir: TestDir,
        private_key: PathBuf,
        config_dir: PathBuf,
    }

    impl Fixture {
        fn new(name: &str) -> Result<Self> {
            let dir = TestDir::new(name)?;
            let private_key = dir.path().join("app.pem");
            std::fs::write(&private_key, generate_private_key(TestKeyFormat::Pkcs1)?)?;
            let config_dir = dir.path().join("gh");
            Ok(Self {
                _dir: dir,
                private_key,
                config_dir,
            })
        }

        fn request(&self, installation_id: Option<u64>) -> RefreshRequest {
            RefreshRequest {
                app_id: 12345,
                installation_id,
                private_key: self.private_key.clone(),
                config_dir: self.config_dir.clone(),
                force: false,
            }
        }

        fn forced_request(&self) -> RefreshRequest {
            RefreshRequest {
                force: true,
                ..self.request(Some(42))
            }
        }

        /// Refreshes 10 minutes after the first mint, when the token alone would be kept.
        fn assert_refresh_mints(&self, request: &RefreshRequest) -> Result<()> {
            let api = FakeApi::default();

            refresh(request, &api, NOW + 600)?;

            assert_eq!(api.token_requests.borrow().len(), 1);
            status(&self.config_dir, NOW + 600)?;
            Ok(())
        }

        fn hosts(&self) -> Result<String> {
            Ok(std::fs::read_to_string(self.config_dir.join("hosts.yml"))?)
        }
    }
}
