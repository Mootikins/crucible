use crucible_daemon::{SessionId, SessionType};
use std::sync::{Mutex, OnceLock};

mod list;
mod misc;
mod reindex;
mod search;
mod show;

pub(super) fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// Materialize a session directory holding the real wire-format log.
///
/// Reads `assets/fixtures/session_log_wire.jsonl`, which
/// `crucible-daemon`'s `the_committed_session_log_is_what_the_daemon_writes`
/// keeps equal to live daemon output. The `SessionWriter`-driven fixture this
/// replaces wrote `LogEvent`, a shape no production path ever appended, so
/// `cru session show` was green against a format it would never meet.
pub(super) async fn setup_test_session(sessions_dir: &std::path::Path) -> SessionId {
    let id = SessionId::new(SessionType::Chat, chrono::Utc::now());
    let session_dir = sessions_dir.join(id.as_str());
    tokio::fs::create_dir_all(&session_dir).await.unwrap();

    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/fixtures/session_log_wire.jsonl");
    let log = tokio::fs::read_to_string(&fixture)
        .await
        .unwrap_or_else(|e| panic!("read {}: {e}", fixture.display()));
    tokio::fs::write(session_dir.join("session.jsonl"), log)
        .await
        .unwrap();

    id
}
