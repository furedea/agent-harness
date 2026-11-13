#![allow(
    clippy::disallowed_methods,
    clippy::disallowed_types,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "integration tests build fixtures on disk and fail fast on any error"
)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Result, ensure};

#[test]
fn complete_source_install_matches_every_golden_file_and_byte() -> Result<()> {
    let prefix = TemporaryPrefix::create()?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"));
    let output = Command::new(env!("CARGO_BIN_EXE_agent-harness"))
        .arg("install")
        .arg("--source")
        .arg(repository.join("tests/fixtures/complete-source"))
        .arg("--prefix")
        .arg(&prefix.0)
        .output()?;
    assert!(
        output.status.success(),
        "install failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let golden = repository.join("tests/golden/complete_source");
    let expected_files = relative_files(&golden)?;
    let actual_files = relative_files(&prefix.0)?;
    assert!(
        !expected_files.is_empty(),
        "golden snapshot must not be empty"
    );
    assert_eq!(actual_files, expected_files, "installed file set changed");
    for path in expected_files {
        // No normalization: the default runtime root uses literal home references,
        // so neither the temporary prefix nor the checkout path enters the output.
        assert_eq!(
            std::fs::read(prefix.0.join(&path))?,
            std::fs::read(golden.join(&path))?,
            "installed bytes changed: {}",
            path.display(),
        );
    }
    Ok(())
}

fn relative_files(root: &Path) -> Result<BTreeSet<PathBuf>> {
    let mut files = BTreeSet::new();
    let mut directories = vec![root.to_path_buf()];
    while let Some(directory) = directories.pop() {
        let (subdirectories, regular_files) = list_directory(&directory)?;
        directories.extend(subdirectories);
        files.extend(relative_to(root, &regular_files)?);
    }
    Ok(files)
}

fn relative_to(root: &Path, paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
    paths
        .iter()
        .map(|path| Ok(path.strip_prefix(root)?.to_path_buf()))
        .collect()
}

/// Splits `directory` into its subdirectories and regular files, rejecting any other entry kind.
fn list_directory(directory: &Path) -> Result<(Vec<PathBuf>, Vec<PathBuf>)> {
    let entries = std::fs::read_dir(directory)?
        .map(|entry| {
            let entry = entry?;
            Ok((entry.path(), entry.file_type()?))
        })
        .collect::<Result<Vec<_>>>()?;
    for (path, kind) in &entries {
        ensure!(
            kind.is_dir() || kind.is_file(),
            "unexpected non-file: {}",
            path.display()
        );
    }
    let (subdirectories, regular_files): (Vec<_>, Vec<_>) =
        entries.into_iter().partition(|(_, kind)| kind.is_dir());
    let paths = |entries: Vec<(PathBuf, _)>| entries.into_iter().map(|(path, _)| path).collect();
    Ok((paths(subdirectories), paths(regular_files)))
}

struct TemporaryPrefix(PathBuf);

impl TemporaryPrefix {
    fn create() -> Result<Self> {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = std::env::temp_dir().join(format!(
            "agent-harness-golden-{}-{nonce}",
            std::process::id(),
        ));
        std::fs::create_dir(&path)?;
        Ok(Self(path))
    }
}

impl Drop for TemporaryPrefix {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
