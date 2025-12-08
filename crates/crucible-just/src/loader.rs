//! Load justfile by executing `just --dump --dump-format json`

use crate::{Justfile, JustError, Result};
use std::path::Path;
use tokio::process::Command;

/// Load justfile from a directory by running `just --dump --dump-format json`
pub async fn load_justfile(dir: &Path) -> Result<Justfile> {
    let output = Command::new("just")
        .args(["--dump", "--dump-format", "json"])
        .current_dir(dir)
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(JustError::CommandError(stderr.to_string()));
    }

    let json = String::from_utf8_lossy(&output.stdout);
    Justfile::from_json(&json).map_err(JustError::from)
}

/// Synchronous version for non-async contexts
pub fn load_justfile_sync(dir: &Path) -> Result<Justfile> {
    use std::process::Command;

    let output = Command::new("just")
        .args(["--dump", "--dump-format", "json"])
        .current_dir(dir)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(JustError::CommandError(stderr.to_string()));
    }

    let json = String::from_utf8_lossy(&output.stdout);
    Justfile::from_json(&json).map_err(JustError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_load_justfile_sync() {
        // This test requires `just` to be installed and a justfile in the repo root
        let repo_root = env::var("CARGO_MANIFEST_DIR")
            .map(|p| Path::new(&p).parent().unwrap().parent().unwrap().to_path_buf())
            .unwrap();

        if repo_root.join("justfile").exists() {
            let jf = load_justfile_sync(&repo_root).unwrap();
            assert!(!jf.recipes.is_empty());
        }
    }
}
