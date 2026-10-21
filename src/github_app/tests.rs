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
        self.fail_with
            .map_or(Ok(()), |message| Err(anyhow!(message)))?;
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

    assert!(api.token_requests.borrow().is_empty());
    Ok(())
}

#[test]
fn refresh_mints_when_an_installation_id_is_configured_after_none() -> Result<()> {
    let fixture = Fixture::new("unset-to-set")?;
    let single = FakeApi {
        installations: vec![42],
        ..FakeApi::default()
    };
    refresh(&fixture.request(None), &single, NOW)?;

    fixture.assert_refresh_mints(&fixture.request(Some(42)))
}

#[test]
fn refresh_resolves_the_installation_again_when_the_id_is_unset() -> Result<()> {
    let fixture = Fixture::new("set-to-unset")?;
    refresh(&fixture.request(Some(42)), &FakeApi::default(), NOW)?;
    let single = FakeApi {
        installations: vec![43],
        ..FakeApi::default()
    };

    refresh(&fixture.request(None), &single, NOW + 600)?;

    assert_eq!(single.token_requests.borrow()[0].1, 43);
    assert!(fixture.hosts()?.contains("ghs_for_43"));
    Ok(())
}

#[test]
fn refresh_fails_when_the_id_is_unset_and_the_app_has_several_installations() -> Result<()> {
    let fixture = Fixture::new("set-to-unset-several")?;
    refresh(&fixture.request(Some(42)), &FakeApi::default(), NOW)?;
    let several = FakeApi {
        installations: vec![42, 43],
        ..FakeApi::default()
    };

    let error = refresh(&fixture.request(None), &several, NOW + 600).unwrap_err();

    assert!(format!("{error:#}").contains("installed 2 times"));
    assert!(several.token_requests.borrow().is_empty());
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
