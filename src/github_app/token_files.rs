use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::fs_ops;
use crate::github_app::api::InstallationToken;
use crate::github_app::timestamp;

const HOSTS_FILE: &str = "hosts.yml";
const CONFIG_FILE: &str = "config.yml";
const STATUS_FILE: &str = "agent_harness_token.json";
/// gh skips its multi-account migration, which needs `GET /user`, once the config is at version 1.
const CONFIG_CONTENT: &str = "version: \"1\"\n";
/// Git expects this user name with installation access tokens.
const TOKEN_USER: &str = "x-access-token";

/// Files that gh reads from `GH_CONFIG_DIR`, plus the refresh status kept beside them.
pub(crate) struct TokenFiles {
    config_dir: PathBuf,
}

/// Last refresh outcome recorded beside the token.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RefreshStatus {
    pub(crate) token: Option<TokenExpiry>,
    pub(crate) last_failure: Option<RefreshFailure>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct TokenExpiry {
    pub(crate) refreshed_at_unix: u64,
    pub(crate) expires_at: String,
    pub(crate) expires_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RefreshFailure {
    pub(crate) failed_at_unix: u64,
    pub(crate) message: String,
}

impl TokenFiles {
    pub(crate) fn new(config_dir: &Path) -> Self {
        Self {
            config_dir: config_dir.to_path_buf(),
        }
    }

    /// Writes the token where gh and `gh auth git-credential` read it and records its expiry.
    pub(crate) fn write_token(&self, token: &InstallationToken, now_unix: u64) -> Result<()> {
        let expiry = TokenExpiry {
            refreshed_at_unix: now_unix,
            expires_at: token.expires_at.clone(),
            expires_at_unix: timestamp::parse_utc(&token.expires_at)?,
        };
        self.create_config_dir()?;
        fs_ops::write_file_atomically(&self.path(HOSTS_FILE), hosts_yaml(&token.token).as_bytes())?;
        self.write_config_once()?;
        self.write_status(&RefreshStatus {
            token: Some(expiry),
            last_failure: None,
        })
    }

    /// Records a failed refresh without touching the current token.
    pub(crate) fn record_failure(&self, message: &str, now_unix: u64) -> Result<()> {
        let mut status = self.status()?.unwrap_or_default();
        status.last_failure = Some(RefreshFailure {
            failed_at_unix: now_unix,
            message: message.to_owned(),
        });
        self.create_config_dir()?;
        self.write_status(&status)
    }

    pub(crate) fn status(&self) -> Result<Option<RefreshStatus>> {
        let path = self.path(STATUS_FILE);
        match std::fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str(&content)
                .map(Some)
                .with_context(|| format!("failed to parse {}", path.display())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error).with_context(|| format!("failed to read {}", path.display())),
        }
    }

    fn write_status(&self, status: &RefreshStatus) -> Result<()> {
        let content = serde_json::to_string_pretty(status)? + "\n";
        fs_ops::write_file_atomically(&self.path(STATUS_FILE), content.as_bytes())
    }

    fn write_config_once(&self) -> Result<()> {
        let path = self.path(CONFIG_FILE);
        if path.exists() {
            return Ok(());
        }
        fs_ops::write_file_atomically(&path, CONFIG_CONTENT.as_bytes())
    }

    fn create_config_dir(&self) -> Result<()> {
        let mut builder = std::fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder
            .create(&self.config_dir)
            .with_context(|| format!("failed to create directory {}", self.config_dir.display()))
    }

    fn path(&self, file: &str) -> PathBuf {
        self.config_dir.join(file)
    }
}

fn hosts_yaml(token: &str) -> String {
    format!(
        "github.com:\n    users:\n        {TOKEN_USER}:\n            oauth_token: {token}\n    \
         git_protocol: https\n    oauth_token: {token}\n    user: {TOKEN_USER}\n"
    )
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;
    use crate::github_app::test_support::TestDir;

    #[test]
    fn write_token_writes_gh_hosts_and_expiry() -> Result<()> {
        let dir = TestDir::new("write-token")?;
        let config_dir = dir.path().join("gh");
        let files = TokenFiles::new(&config_dir);

        files.write_token(&token("ghs_first", "2026-10-15T13:00:00Z"), 1_792_069_000)?;

        let hosts: Value =
            serde_norway::from_str(&std::fs::read_to_string(config_dir.join("hosts.yml"))?)?;
        assert_eq!(hosts["github.com"]["oauth_token"], "ghs_first");
        assert_eq!(hosts["github.com"]["user"], "x-access-token");
        assert_eq!(
            hosts["github.com"]["users"]["x-access-token"]["oauth_token"],
            "ghs_first"
        );
        assert_eq!(hosts["github.com"]["git_protocol"], "https");
        assert_eq!(
            std::fs::read_to_string(config_dir.join("config.yml"))?,
            "version: \"1\"\n"
        );
        assert_eq!(
            files.status()?,
            Some(RefreshStatus {
                token: Some(TokenExpiry {
                    refreshed_at_unix: 1_792_069_000,
                    expires_at: "2026-10-15T13:00:00Z".to_owned(),
                    expires_at_unix: 1_792_069_200,
                }),
                last_failure: None,
            })
        );
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn write_token_keeps_the_token_private() -> Result<()> {
        use std::os::unix::fs::PermissionsExt;

        let dir = TestDir::new("token-mode")?;
        let config_dir = dir.path().join("gh");
        let files = TokenFiles::new(&config_dir);

        files.write_token(&token("ghs_first", "2026-10-15T13:00:00Z"), 0)?;

        let mode = |path: &Path| -> Result<u32> {
            Ok(std::fs::metadata(path)?.permissions().mode() & 0o777)
        };
        assert_eq!(mode(&config_dir.join("hosts.yml"))?, 0o600);
        assert_eq!(mode(&config_dir)?, 0o700);
        Ok(())
    }

    #[test]
    fn write_token_replaces_the_previous_token_and_keeps_existing_config() -> Result<()> {
        let dir = TestDir::new("replace-token")?;
        let config_dir = dir.path().join("gh");
        std::fs::create_dir_all(&config_dir)?;
        std::fs::write(
            config_dir.join("config.yml"),
            "version: \"1\"\neditor: vim\n",
        )?;
        let files = TokenFiles::new(&config_dir);
        files.write_token(&token("ghs_first", "2026-10-15T13:00:00Z"), 0)?;

        files.write_token(&token("ghs_second", "2026-10-15T14:00:00Z"), 0)?;

        let hosts = std::fs::read_to_string(config_dir.join("hosts.yml"))?;
        assert!(hosts.contains("ghs_second"));
        assert!(!hosts.contains("ghs_first"));
        assert_eq!(
            std::fs::read_to_string(config_dir.join("config.yml"))?,
            "version: \"1\"\neditor: vim\n"
        );
        Ok(())
    }

    #[test]
    fn record_failure_keeps_the_token_and_its_expiry() -> Result<()> {
        let dir = TestDir::new("record-failure")?;
        let config_dir = dir.path().join("gh");
        let files = TokenFiles::new(&config_dir);
        files.write_token(&token("ghs_first", "2026-10-15T13:00:00Z"), 10)?;

        files.record_failure("network down", 20)?;

        let status = files.status()?.unwrap_or_default();
        assert_eq!(status.token.map(|token| token.refreshed_at_unix), Some(10));
        assert_eq!(
            status.last_failure,
            Some(RefreshFailure {
                failed_at_unix: 20,
                message: "network down".to_owned(),
            })
        );
        assert!(std::fs::read_to_string(config_dir.join("hosts.yml"))?.contains("ghs_first"));
        Ok(())
    }

    fn token(token: &str, expires_at: &str) -> InstallationToken {
        InstallationToken {
            token: token.to_owned(),
            expires_at: expires_at.to_owned(),
        }
    }
}
