//! Developer-local test settings, read from `.env.local` at the repo root.
//!
//! Some tests need a real LLM endpoint. Hardcoding one is how
//! `llm_backend_comparison` ended up pointing at `https://llm.example.com` — a
//! placeholder domain, so those tests could not pass on any machine, including
//! the one that wrote them. Requiring the developer to export a variable by
//! hand before every run is the other failure mode: the tests are `#[ignore]`d,
//! so nobody notices the variable is missing and they quietly never run.
//!
//! A gitignored `.env.local` fixes both. It is committed nowhere (`.gitignore`
//! has covered `.env.local` since before this module existed), each developer
//! points it at whatever they actually run, and a test with no setting says so
//! and returns rather than failing.
//!
//! See `.env.local.example` for the keys.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Repo root, resolved from this crate's manifest directory.
///
/// `CARGO_MANIFEST_DIR` expands where it is written — here, in
/// `crates/crucible-core` — so two levels up is the workspace root regardless
/// of which crate's tests are calling.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Parsed `.env.local`, read once per process.
fn local_env() -> &'static Vec<(String, String)> {
    static CACHE: OnceLock<Vec<(String, String)>> = OnceLock::new();
    CACHE.get_or_init(|| {
        let Ok(text) = std::fs::read_to_string(repo_root().join(".env.local")) else {
            return Vec::new();
        };
        text.lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .filter_map(|l| l.split_once('='))
            .map(|(k, v)| {
                // Strip one layer of matching quotes, the way a shell would —
                // a value pasted with quotes should not arrive with them.
                let v = v.trim();
                let v = v
                    .strip_prefix('"')
                    .and_then(|v| v.strip_suffix('"'))
                    .or_else(|| v.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
                    .unwrap_or(v);
                (k.trim().to_string(), v.to_string())
            })
            .collect()
    })
}

/// A test setting: the real environment first, then `.env.local`.
///
/// The environment wins so a one-off run can override the file without editing
/// it — `CRUCIBLE_TEST_LLM_ENDPOINT=… cargo nextest run …` behaves the way
/// anyone would expect.
pub fn test_env(key: &str) -> Option<String> {
    if let Ok(v) = std::env::var(key) {
        if !v.is_empty() {
            return Some(v);
        }
    }
    local_env()
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.clone())
        .filter(|v| !v.is_empty())
}

/// Fetch a required setting, or print why the test is being skipped.
///
/// Returns `None` after printing, so a caller reads as:
///
/// ```ignore
/// let Some(endpoint) = require_test_env("CRUCIBLE_TEST_LLM_ENDPOINT") else { return };
/// ```
///
/// Printing rather than failing is deliberate: these tests are `#[ignore]`d and
/// opt-in, so a machine without the setting has not done anything wrong. A
/// silent `return` would be worse — it looks like a pass.
pub fn require_test_env(key: &str) -> Option<String> {
    match test_env(key) {
        Some(v) => Some(v),
        None => {
            eprintln!(
                "SKIPPED: {key} is not set. Add it to .env.local at the repo root \
                 (see .env.local.example) or export it for this run."
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The repo root is where `.env.local` and `.env.local.example` live, and
    /// resolving it wrong is invisible: every lookup just returns `None` and
    /// every dependent test skips itself while looking fine.
    #[test]
    fn repo_root_is_the_workspace_root() {
        assert!(
            repo_root().join("Cargo.toml").is_file(),
            "expected a workspace Cargo.toml at {}",
            repo_root().display()
        );
        assert!(repo_root().join("crates").is_dir());
    }

    #[test]
    fn an_unset_key_is_none_rather_than_empty() {
        assert_eq!(test_env("CRUCIBLE_TEST_KEY_THAT_DOES_NOT_EXIST"), None);
    }

    /// The environment beating the file is what makes a one-off override work.
    #[test]
    fn the_environment_wins_over_the_file() {
        let _guard = crate::test_support::EnvVarGuard::set(
            "CRUCIBLE_TEST_LOCAL_ENV_PRECEDENCE",
            "from-env".to_string(),
        );
        assert_eq!(
            test_env("CRUCIBLE_TEST_LOCAL_ENV_PRECEDENCE").as_deref(),
            Some("from-env")
        );
    }

    /// An empty value is not a value. Left as `Some("")` it would reach a
    /// provider as an empty endpoint and fail somewhere far from here.
    #[test]
    fn an_empty_value_reads_as_unset() {
        let _guard =
            crate::test_support::EnvVarGuard::set("CRUCIBLE_TEST_LOCAL_ENV_EMPTY", String::new());
        assert_eq!(test_env("CRUCIBLE_TEST_LOCAL_ENV_EMPTY"), None);
    }
}
