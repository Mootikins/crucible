use crucible_daemon::{LogEvent, SessionId, SessionType, SessionWriter};
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

pub(super) async fn setup_test_session(sessions_dir: &std::path::Path) -> SessionId {
    let mut writer = SessionWriter::create(sessions_dir, SessionType::Chat)
        .await
        .unwrap();
    writer
        .append(LogEvent::system("You are helpful"))
        .await
        .unwrap();
    writer
        .append(LogEvent::user("Hello, how are you?"))
        .await
        .unwrap();
    writer
        .append(LogEvent::assistant("I'm doing well, thanks!"))
        .await
        .unwrap();
    writer.id().clone()
}
