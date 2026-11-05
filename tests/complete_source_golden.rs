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
        for entry in std::fs::read_dir(directory)? {
            let entry = entry?;
            let path = entry.path();
            let kind = entry.file_type()?;
            if kind.is_dir() {
                directories.push(path);
            } else {
                ensure!(kind.is_file(), "unexpected non-file: {}", path.display());
                files.insert(path.strip_prefix(root)?.to_path_buf());
            }
        }
    }
    Ok(files)
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
