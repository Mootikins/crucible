//! Browser sessions minted by `POST /api/auth/login`.
//!
//! Persisted beside the API key, because `cru web` is a desktop process that
//! gets restarted constantly and the browsers holding its sessions are on
//! other devices. A store that lives only in memory silently turns every
//! restart into a re-login on every phone and tablet on the LAN.

use super::api_key::{config_file, write_private};
use super::constant_time_eq;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// How long a session cookie stays valid, and the `Max-Age` the cookie itself
/// carries.
///
/// 30 days, matching the pre-hardening lifetime. A shorter window is not the
/// mitigation it looks like here: the token is stored 0600 in the same
/// directory as the API key it was minted from, so an attacker who can read it
/// can read the key too and does not need the session at all. What actually
/// bounds the damage is that the token is revocable (`POST /api/auth/logout`),
/// dies with the key that minted it (`cru web key --rotate`), and is capped at
/// [`MAX_SESSIONS`]. Against that, a 7-day window buys one extra re-login per
/// week on every tablet in the house and nothing else.
pub const SESSION_TTL: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// Cap on live sessions, so a login endpoint that anyone with the key can hit
/// cannot grow the process's memory without bound. Oldest is evicted first.
const MAX_SESSIONS: usize = 64;

#[derive(Serialize, Deserialize)]
struct Session {
    token: String,
    /// The API key this session was minted from.
    ///
    /// This does NOT make `cru web key --rotate` retire live sessions: nothing
    /// changes `ApiKeyState.api_key` in a running process, so the comparison
    /// never fails until a restart — which drops the store anyway. What it
    /// buys is that a session restored from disk cannot outlive the key that
    /// minted it, across a restart or a config reload.
    minted_from: String,
    /// Absolute expiry, seconds since the Unix epoch.
    ///
    /// Wall clock, not `Instant`: a monotonic clock has no meaning across a
    /// restart, and on Linux it excludes suspend, so a laptop asleep for a
    /// week would hand every one of its sessions a week of extra life.
    expires_at: u64,
}

/// Path of the persisted session file (`~/.config/crucible/sessions.json`).
pub fn sessions_path() -> Option<PathBuf> {
    config_file("sessions.json")
}

/// Browser sessions minted by `POST /api/auth/login`.
///
/// The cookie carries one of these tokens, never the API key itself: tokens
/// expire, can be dropped individually (`POST /api/auth/logout`), and are bound
/// to the key that minted them.
#[derive(Clone, Default)]
pub struct SessionStore {
    sessions: Arc<Mutex<Vec<Session>>>,
    /// Where the sessions are kept across restarts. `None` — the [`Default`] —
    /// is memory-only: nothing on disk is read or written.
    path: Option<PathBuf>,
}

impl SessionStore {
    /// Sessions persisted at `path`, loaded now and rewritten on every mint and
    /// revoke. `None` keeps them in memory only.
    ///
    /// Tests MUST pass a `TempDir`-rooted path (or `None`): the production path
    /// is the operator's real session file, which a test may neither read nor
    /// overwrite.
    pub fn persistent(path: Option<PathBuf>) -> Self {
        let sessions = path.as_deref().map(load).unwrap_or_default();
        Self {
            sessions: Arc::new(Mutex::new(sessions)),
            path,
        }
    }

    /// Mint a session bound to `api_key` and return its token.
    pub fn issue(&self, api_key: &str) -> String {
        use rand::RngExt;
        let token: String = rand::rng()
            .sample_iter(rand::distr::Alphanumeric)
            .take(32)
            .map(char::from)
            .collect();

        let mut sessions = self.lock_live();
        while sessions.len() >= MAX_SESSIONS {
            sessions.remove(0);
        }
        sessions.push(Session {
            token: token.clone(),
            minted_from: api_key.to_string(),
            expires_at: now_secs() + SESSION_TTL.as_secs(),
        });
        self.persist(&sessions);
        token
    }

    /// Whether `token` names a live session minted from `api_key`.
    ///
    /// Compares every candidate without short-circuiting: `&` rather than `&&`,
    /// and no early `return`, so the answer costs the same however many leading
    /// bytes of a guess happen to be right.
    pub fn verify(&self, token: &str, api_key: &str) -> bool {
        let sessions = self.lock_live();
        let mut valid = false;
        for session in sessions.iter() {
            valid |= constant_time_eq(token.as_bytes(), session.token.as_bytes())
                & constant_time_eq(api_key.as_bytes(), session.minted_from.as_bytes());
        }
        valid
    }

    /// Drop the session named by `token` (logout).
    pub fn revoke(&self, token: &str) {
        let mut sessions = self.lock_live();
        sessions.retain(|s| !constant_time_eq(token.as_bytes(), s.token.as_bytes()));
        self.persist(&sessions);
    }

    /// Take the lock and drop whatever has expired. Expiry is not written back
    /// on its own — a load prunes too, so the file converges without an I/O
    /// round-trip on the request path.
    fn lock_live(&self) -> std::sync::MutexGuard<'_, Vec<Session>> {
        let mut sessions = self.sessions.lock().expect("session store poisoned");
        let now = now_secs();
        sessions.retain(|s| s.expires_at > now);
        sessions
    }

    fn persist(&self, sessions: &[Session]) {
        let Some(path) = self.path.as_deref() else {
            return;
        };
        // A session file we cannot write costs a re-login after the next
        // restart; it must never cost a failed login now.
        if let Err(error) = serde_json::to_vec(sessions)
            .map_err(std::io::Error::other)
            .and_then(|encoded| write_private(path, &encoded))
        {
            tracing::warn!(%error, path = %path.display(), "Could not persist browser sessions");
        }
    }
}

fn load(path: &Path) -> Vec<Session> {
    let Ok(raw) = std::fs::read(path) else {
        return Vec::new();
    };
    let mut sessions: Vec<Session> = match serde_json::from_slice(&raw) {
        Ok(sessions) => sessions,
        Err(error) => {
            tracing::warn!(%error, path = %path.display(), "Ignoring unreadable session file");
            return Vec::new();
        }
    };
    let now = now_secs();
    sessions.retain(|s| s.expires_at > now);
    // A file that somehow carries more than the cap must not raise it.
    if sessions.len() > MAX_SESSIONS {
        sessions.drain(..sessions.len() - MAX_SESSIONS);
    }
    sessions
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A store rooted in `dir`, as if the process had just started.
    fn store_at(dir: &tempfile::TempDir) -> SessionStore {
        SessionStore::persistent(Some(dir.path().join("sessions.json")))
    }

    fn persisted(dir: &tempfile::TempDir) -> serde_json::Value {
        let raw = std::fs::read_to_string(dir.path().join("sessions.json"))
            .expect("sessions file written");
        serde_json::from_str(&raw).expect("sessions file is JSON")
    }

    #[test]
    fn a_session_survives_a_restart_and_stays_bound_to_the_minting_key() {
        // `cru web` is a locally-restarted desktop process. Losing every
        // session on restart means every phone and tablet on the LAN retypes a
        // 32-char key — which is what the cookie's Max-Age promises it will
        // not have to do.
        let dir = tempfile::tempdir().unwrap();
        let token = store_at(&dir).issue("secret-key");

        let restarted = store_at(&dir);
        assert!(restarted.verify(&token, "secret-key"));
        assert!(
            !restarted.verify(&token, "rotated-key"),
            "rotation must still retire every outstanding session"
        );
    }

    #[test]
    fn a_revoked_session_does_not_come_back_after_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_at(&dir);
        let kept = store.issue("secret-key");
        let dropped = store.issue("secret-key");
        store.revoke(&dropped);

        let restarted = store_at(&dir);
        assert!(!restarted.verify(&dropped, "secret-key"));
        assert!(restarted.verify(&kept, "secret-key"));
    }

    #[test]
    fn session_expiry_is_absolute_wall_clock_time() {
        // Not `Instant`: on Linux the monotonic clock excludes suspend, so a
        // laptop asleep for a week would silently extend every session by a
        // week. The persisted expiry is a Unix timestamp for the same reason —
        // it has to mean something across process restarts.
        let dir = tempfile::tempdir().unwrap();
        store_at(&dir).issue("secret-key");

        let expires_at = persisted(&dir)[0]["expires_at"].as_u64().expect("expiry");
        let expected = now_secs() + SESSION_TTL.as_secs();
        assert!(
            expires_at.abs_diff(expected) <= 5,
            "expiry {expires_at} should be ~{expected} (now + TTL)"
        );
    }

    #[test]
    fn an_expired_session_is_not_restored() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_at(&dir);
        let token = store.issue("secret-key");

        // Rewrite the persisted expiry into the past — the shape a store
        // loaded a month after the last login has.
        let mut entries = persisted(&dir);
        entries[0]["expires_at"] = serde_json::json!(now_secs() - 1);
        std::fs::write(
            dir.path().join("sessions.json"),
            serde_json::to_string(&entries).unwrap(),
        )
        .unwrap();

        assert!(!store_at(&dir).verify(&token, "secret-key"));
    }

    #[test]
    fn a_corrupt_sessions_file_starts_an_empty_store_instead_of_failing() {
        // A truncated write must cost a re-login, never a server that refuses
        // to start.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("sessions.json"), "[{\"token\":").unwrap();

        let store = store_at(&dir);
        let token = store.issue("secret-key");
        assert!(store.verify(&token, "secret-key"));
    }

    #[cfg(unix)]
    #[test]
    fn the_persisted_sessions_file_is_readable_only_by_its_owner() {
        // The file holds live credentials, exactly like the api_key beside it.
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        store_at(&dir).issue("secret-key");

        let mode = std::fs::metadata(dir.path().join("sessions.json"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn a_store_without_a_path_persists_nothing() {
        // The default: tests and any caller that has not opted in must never
        // touch the real `~/.config/crucible/sessions.json`.
        let store = SessionStore::default();
        let token = store.issue("secret-key");
        assert!(store.verify(&token, "secret-key"));
        assert!(store.path.is_none());
    }

    #[test]
    fn a_session_expires_and_is_never_valid_for_another_key() {
        let store = SessionStore::default();
        let token = store.issue("secret-key");
        assert!(store.verify(&token, "secret-key"));
        assert!(!store.verify(&token, "rotated-key"));
        assert!(!store.verify("not-a-token", "secret-key"));

        store.revoke(&token);
        assert!(!store.verify(&token, "secret-key"));
    }

    #[test]
    fn the_session_store_evicts_rather_than_growing_without_bound() {
        // Login is reachable by anyone holding the key; a client that never
        // logs out must not be able to grow the process forever.
        let store = SessionStore::default();
        let first = store.issue("secret-key");
        for _ in 0..MAX_SESSIONS {
            store.issue("secret-key");
        }
        assert!(!store.verify(&first, "secret-key"));
        assert_eq!(
            store.sessions.lock().unwrap().len(),
            MAX_SESSIONS,
            "the store is capped"
        );
    }
}
