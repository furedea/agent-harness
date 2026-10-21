use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result, bail};

#[derive(Clone, Copy)]
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
