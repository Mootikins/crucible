//! Where the API key comes from, and how a presented one is checked.

use super::{constant_time_eq, ApiKeyState};

/// Constant-time check of a provided key against the configured one.
/// `None` configured key means auth is disabled — everything verifies.
pub fn verify_api_key(state: &ApiKeyState, provided: &str) -> bool {
    match &state.api_key {
        Some(expected) => constant_time_eq(provided.as_bytes(), expected.as_bytes()),
        None => true,
    }
}

/// Load API key from config, fall back to file, or generate a new one.
///
/// Resolution order:
/// 1. Explicit key from `WebConfig.api_key` (pass `configured_key`)
///    - Empty string `""` disables auth entirely (returns `None`).
/// 2. Read from `~/.config/crucible/api_key`
/// 3. Generate a random 32-char alphanumeric key and persist it there
pub fn resolve_api_key(configured_key: Option<&str>) -> Option<String> {
    resolve_api_key_at(configured_key, api_key_path())
}

/// [`resolve_api_key`] with an injectable key-file path.
///
/// Tests MUST use this with a TempDir-rooted path: the default path is the
/// developer's real `~/.config/crucible/api_key`, and the fallback both
/// READS that credential and, when absent/empty, WRITES a generated one —
/// neither may ever happen from a test.
pub fn resolve_api_key_at(
    configured_key: Option<&str>,
    key_path: Option<std::path::PathBuf>,
) -> Option<String> {
    match configured_key {
        // Explicitly set to empty string — auth disabled.
        Some("") => return None,
        // Explicitly set to a value — use it.
        Some(key) => return Some(key.to_string()),
        None => {}
    }

    let key_path = key_path?;

    if key_path.exists() {
        let contents = std::fs::read_to_string(&key_path).ok()?;
        let trimmed = contents.trim().to_string();
        if !trimmed.is_empty() {
            return Some(trimmed);
        }
    }

    generate_and_persist_key(&key_path)
}

/// Path of the persisted API key file (`~/.config/crucible/api_key`).
pub fn api_key_path() -> Option<std::path::PathBuf> {
    config_file("api_key")
}

/// A file in Crucible's config directory (`~/.config/crucible/<name>`).
pub(super) fn config_file(name: &str) -> Option<std::path::PathBuf> {
    Some(dirs::config_dir()?.join("crucible").join(name))
}

/// Write `contents` to `path`, creating parent directories and (on unix)
/// keeping the file readable only by its owner.
///
/// Shared by the API key and the session store: both hold live credentials, so
/// there is one place that decides how a credential file is created rather
/// than one per caller to forget the mode in.
pub(super) fn write_private(path: &std::path::Path, contents: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?
            .write_all(contents)
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, contents)
    }
}

/// Generate a fresh random key and persist it (0600 on unix), replacing any
/// existing one. Used at first startup and by `cru web key --rotate`.
pub fn generate_and_persist_key(key_path: &std::path::Path) -> Option<String> {
    use rand::RngExt;
    let key: String = rand::rng()
        .sample_iter(rand::distr::Alphanumeric)
        .take(32)
        .map(char::from)
        .collect();

    write_private(key_path, key.as_bytes()).ok()?;
    tracing::info!("Generated new API key at {}", key_path.display());

    Some(key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::middleware::auth::HostPolicy;

    #[test]
    fn verify_api_key_matches_and_disabled_auth_accepts_all() {
        // `new_at(.., None)` keeps the session store in memory; `new` would persist
        // into the developer's real `~/.config/crucible/sessions.json`.
        let policy = HostPolicy::from_bind("127.0.0.1", 3000, &[]);
        let enabled = ApiKeyState::new_at(Some("secret-key".into()), policy.clone(), None);
        assert!(verify_api_key(&enabled, "secret-key"));
        assert!(!verify_api_key(&enabled, "wrong"));

        let disabled = ApiKeyState::new_at(None, policy, None);
        assert!(verify_api_key(&disabled, "anything"));
    }

    #[test]
    fn resolve_api_key_returns_none_for_empty_string() {
        assert!(resolve_api_key(Some("")).is_none());
    }

    #[test]
    fn resolve_api_key_returns_explicit_value() {
        assert_eq!(resolve_api_key(Some("my-key")), Some("my-key".to_string()));
    }

    // The file-fallback paths are exercised ONLY through the injectable
    // variant — resolve_api_key(None) reads (and can create) the real
    // ~/.config/crucible/api_key, which a test must never touch.
    #[test]
    fn resolve_api_key_at_reads_and_trims_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("api_key");
        std::fs::write(&path, "  stored-key\n").unwrap();
        assert_eq!(
            resolve_api_key_at(None, Some(path)),
            Some("stored-key".to_string())
        );
    }

    #[test]
    fn resolve_api_key_at_generates_and_persists_when_missing_or_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("api_key");

        let generated = resolve_api_key_at(None, Some(path.clone())).expect("generated key");
        assert_eq!(generated.len(), 32);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), generated);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }

        // A second resolve returns the persisted key, not a fresh one.
        assert_eq!(resolve_api_key_at(None, Some(path)), Some(generated));
    }

    #[test]
    fn resolve_api_key_at_without_path_disables_auth() {
        assert_eq!(resolve_api_key_at(None, None), None);
    }
}
