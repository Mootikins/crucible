use crate::config::CliConfig;
use crucible_daemon::{SessionId, SessionType};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

mod list;
mod misc;
mod reindex;
mod show;

/// A CLI config whose session store lives inside `tmp`.
///
/// `data_home`, not `kiln_path`: sessions no longer live inside a kiln, so
/// `io::sessions_dir` reads the data root — and leaving it unset would point
/// these tests at the developer's real `~/.crucible`.
pub(super) fn test_config(tmp: &Path) -> CliConfig {
    CliConfig {
        kiln_path: tmp.to_path_buf(),
        data_home: Some(tmp.to_path_buf()),
        ..Default::default()
    }
}

/// The sessions root [`test_config`] resolves to, created on disk.
pub(super) fn test_sessions_dir(tmp: &Path) -> PathBuf {
    let path = crate::commands::session::io::sessions_dir(&test_config(tmp));
    std::fs::create_dir_all(&path).unwrap();
    path
}

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
    let id = SessionId::generate(SessionType::Chat);
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
