use std::path::Path;

use anyhow::{Context, Result, bail};
use base64::{
    Engine,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use ring::{rand::SystemRandom, signature::RsaKeyPair};
use serde::Serialize;

/// GitHub accepts App JWTs issued up to 60 seconds in the past to absorb clock drift.
const ISSUED_AT_BACKDATE_SECS: u64 = 60;
/// GitHub rejects App JWTs that expire more than 10 minutes after issue.
const LIFETIME_SECS: u64 = 9 * 60;

const PKCS1_LABEL: &str = "RSA PRIVATE KEY";
const PKCS8_LABEL: &str = "PRIVATE KEY";

/// RSA private key that a GitHub App uses to sign its JWTs.
pub(crate) struct AppPrivateKey(RsaKeyPair);

#[derive(Serialize)]
struct Header {
    alg: &'static str,
    typ: &'static str,
}

#[derive(Serialize)]
struct Claims {
    iat: u64,
    exp: u64,
    iss: String,
}

impl AppPrivateKey {
    pub(crate) fn read(path: &Path) -> Result<Self> {
        let pem = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read GitHub App private key {}", path.display()))?;
        Self::from_pem(&pem)
            .with_context(|| format!("invalid GitHub App private key {}", path.display()))
    }

    pub(crate) fn from_pem(pem: &str) -> Result<Self> {
        rsa_key_pair(pem).map(Self)
    }

    #[cfg(test)]
    pub(crate) fn public_key_der(&self) -> &[u8] {
        use ring::signature::KeyPair;

        self.0.public_key().as_ref()
    }
}

/// Builds the RS256 JWT that authenticates as the App itself.
pub(crate) fn app_jwt(app_id: u64, key: &AppPrivateKey, now_unix: u64) -> Result<String> {
    let header = encode_json(&Header {
        alg: "RS256",
        typ: "JWT",
    })?;
    let claims = encode_json(&Claims {
        iat: now_unix.saturating_sub(ISSUED_AT_BACKDATE_SECS),
        exp: now_unix + LIFETIME_SECS,
        iss: app_id.to_string(),
    })?;
    let signing_input = format!("{header}.{claims}");

    let mut signature = vec![0; key.0.public().modulus_len()];
    key.0
        .sign(
            &ring::signature::RSA_PKCS1_SHA256,
            &SystemRandom::new(),
            signing_input.as_bytes(),
            &mut signature,
        )
        .map_err(|error| anyhow::anyhow!("failed to sign GitHub App JWT: {error}"))?;

    Ok(format!(
        "{signing_input}.{}",
        URL_SAFE_NO_PAD.encode(signature)
    ))
}

fn encode_json(value: &impl Serialize) -> Result<String> {
    Ok(URL_SAFE_NO_PAD.encode(serde_json::to_vec(value)?))
}

/// Parses a PKCS#1 (`RSA PRIVATE KEY`, as GitHub issues) or PKCS#8 PEM key.
fn rsa_key_pair(pem: &str) -> Result<RsaKeyPair> {
    let key_pair = if let Some(der) = pem_body(pem, PKCS1_LABEL)? {
        RsaKeyPair::from_der(&der)
    } else if let Some(der) = pem_body(pem, PKCS8_LABEL)? {
        RsaKeyPair::from_pkcs8(&der)
    } else {
        bail!("expected a PEM RSA private key");
    };
    key_pair.map_err(|error| anyhow::anyhow!("unusable RSA private key: {error}"))
}

fn pem_body(pem: &str, label: &str) -> Result<Option<Vec<u8>>> {
    let begin = format!("-----BEGIN {label}-----");
    let end = format!("-----END {label}-----");
    let Some((_, rest)) = pem.split_once(&begin) else {
        return Ok(None);
    };
    let Some((body, _)) = rest.split_once(&end) else {
        bail!("PEM block {label} is not terminated");
    };
    let body: String = body.split_whitespace().collect();
    STANDARD
        .decode(body)
        .map(Some)
        .context("PEM body is not valid base64")
}

#[cfg(test)]
mod tests {
    use ring::signature::{RSA_PKCS1_2048_8192_SHA256, UnparsedPublicKey};
    use serde_json::{Value, json};

    use super::*;
    use crate::github_app::test_support::{TestKeyFormat, generate_private_key};

    #[test]
    fn app_jwt_signs_rs256_claims_for_the_app() -> Result<()> {
        let key = AppPrivateKey::from_pem(&generate_private_key(TestKeyFormat::Pkcs1)?)?;

        let jwt = app_jwt(12345, &key, 1_000_000)?;

        let parts: Vec<&str> = jwt.split('.').collect();
        assert_eq!(parts.len(), 3);
        assert_eq!(
            decode_json(parts[0])?,
            json!({"alg": "RS256", "typ": "JWT"})
        );
        assert_eq!(
            decode_json(parts[1])?,
            json!({"iat": 999_940, "exp": 1_000_540, "iss": "12345"})
        );
        let signature = URL_SAFE_NO_PAD.decode(parts[2])?;
        let signing_input = format!("{}.{}", parts[0], parts[1]);
        UnparsedPublicKey::new(&RSA_PKCS1_2048_8192_SHA256, key.public_key_der())
            .verify(signing_input.as_bytes(), &signature)
            .map_err(|_| anyhow::anyhow!("JWT signature does not verify"))?;
        Ok(())
    }

    #[test]
    fn from_pem_accepts_pkcs8_keys() -> Result<()> {
        let key = AppPrivateKey::from_pem(&generate_private_key(TestKeyFormat::Pkcs8)?)?;

        assert!(app_jwt(1, &key, 1_000).is_ok());
        Ok(())
    }

    #[test]
    fn from_pem_rejects_non_key_input() {
        let error = AppPrivateKey::from_pem("not a key")
            .err()
            .map(|error| error.to_string());

        assert_eq!(error.as_deref(), Some("expected a PEM RSA private key"));
    }

    fn decode_json(part: &str) -> Result<Value> {
        Ok(serde_json::from_slice(&URL_SAFE_NO_PAD.decode(part)?)?)
    }
}
