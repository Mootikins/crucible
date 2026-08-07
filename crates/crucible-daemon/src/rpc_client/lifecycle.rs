//! Daemon lifecycle utilities
//!
//! This module provides utilities for checking daemon status. The actual daemon
//! spawning is handled by `DaemonClient::start_daemon()`, and mutual exclusion
//! between daemons is enforced by the socket flock in `Server::bind`.

use std::path::{Path, PathBuf};
use tracing::debug;

pub fn default_socket_path() -> PathBuf {
    crucible_core::protocol::socket_path()
}

/// Where a background-spawned daemon's stdout/stderr land.
///
/// A spawned daemon is detached from any terminal; before this existed its
/// output went to `Stdio::null()`, so when it died on startup the connect
/// retry loop timed out with no cause to show. Honors `CRUCIBLE_HOME` via
/// [`crucible_core::config::crucible_home`], which keeps tests hermetic.
pub fn daemon_log_path() -> PathBuf {
    crucible_core::config::crucible_home().join("daemon.log")
}

/// Rotate the log once it crosses this size. There is no long-running
/// process to rotate it later — the daemon just inherits the fd — so the
/// spawn moment is the only chance to keep growth bounded.
const LOG_ROTATE_BYTES: u64 = 5 * 1024 * 1024;

/// stdout/stderr handles for spawning a background daemon: the daemon log,
/// or `Stdio::null()` when the log file can't be opened — a daemon that
/// can't log must still be able to start.
pub fn daemon_log_stdio() -> (std::process::Stdio, std::process::Stdio) {
    open_rotating_log(&daemon_log_path())
}

fn open_rotating_log(path: &Path) -> (std::process::Stdio, std::process::Stdio) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    // One `.old` generation: enough to survive a crash-then-restart without
    // losing the crash output, without growing forever.
    if std::fs::metadata(path).is_ok_and(|m| m.len() > LOG_ROTATE_BYTES) {
        let _ = std::fs::rename(path, path.with_extension("log.old"));
    }
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path);
    match file {
        Ok(f) => {
            let err = f
                .try_clone()
                .map(std::process::Stdio::from)
                .unwrap_or_else(|_| std::process::Stdio::null());
            (std::process::Stdio::from(f), err)
        }
        Err(_) => (std::process::Stdio::null(), std::process::Stdio::null()),
    }
}

/// Last `n` lines of `path`, or `None` when the file is missing, unreadable,
/// or effectively empty.
pub fn read_log_tail(path: &Path, n: usize) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    let lines: Vec<&str> = content.lines().collect();
    let start = lines.len().saturating_sub(n);
    let tail = lines[start..].join("\n");
    if tail.trim().is_empty() {
        None
    } else {
        Some(tail)
    }
}

/// Check if daemon is running (socket exists and accepts connections)
pub fn is_daemon_running(socket: &Path) -> bool {
    if !socket.exists() {
        return false;
    }

    match std::os::unix::net::UnixStream::connect(socket) {
        Ok(_) => true,
        Err(e) => {
            debug!("Socket exists but connection failed: {}", e);
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_default_socket_path_ends_with_sock() {
        let path = default_socket_path();
        assert!(path.ends_with("crucible.sock"));
    }

    #[test]
    fn test_default_socket_path_is_absolute() {
        let path = default_socket_path();
        assert!(path.is_absolute());
    }

    #[test]
    fn test_is_daemon_running_false_when_no_socket() {
        let tmp = TempDir::new().unwrap();
        let socket = tmp.path().join("nonexistent.sock");
        assert!(!is_daemon_running(&socket));
    }

    #[test]
    fn test_is_daemon_running_false_when_socket_file_exists_but_not_listening() {
        let tmp = TempDir::new().unwrap();
        let socket = tmp.path().join("fake.sock");
        std::fs::write(&socket, "not a socket").unwrap();
        assert!(!is_daemon_running(&socket));
    }

    #[test]
    fn log_tail_returns_exactly_the_last_n_lines() {
        let tmp = TempDir::new().unwrap();
        let log = tmp.path().join("daemon.log");
        std::fs::write(&log, "one\ntwo\nthree\nfour\n").unwrap();

        assert_eq!(read_log_tail(&log, 2).as_deref(), Some("three\nfour"));
    }

    #[test]
    fn log_tail_returns_the_whole_file_when_shorter_than_n() {
        let tmp = TempDir::new().unwrap();
        let log = tmp.path().join("daemon.log");
        std::fs::write(&log, "only line\n").unwrap();

        assert_eq!(read_log_tail(&log, 50).as_deref(), Some("only line"));
    }

    #[test]
    fn an_oversized_log_is_rotated_at_spawn_time() {
        let tmp = TempDir::new().unwrap();
        let log = tmp.path().join("daemon.log");
        std::fs::write(&log, vec![b'x'; (LOG_ROTATE_BYTES + 1) as usize]).unwrap();

        let _stdio = open_rotating_log(&log);

        assert!(
            tmp.path().join("daemon.log.old").exists(),
            "the oversized log must be rotated aside"
        );
        assert!(
            std::fs::metadata(&log).unwrap().len() < LOG_ROTATE_BYTES,
            "the live log must start fresh after rotation"
        );
    }

    #[test]
    fn a_small_log_is_not_rotated() {
        let tmp = TempDir::new().unwrap();
        let log = tmp.path().join("daemon.log");
        std::fs::write(&log, "recent output\n").unwrap();

        let _stdio = open_rotating_log(&log);

        assert!(!tmp.path().join("daemon.log.old").exists());
        assert_eq!(
            std::fs::read_to_string(&log).unwrap(),
            "recent output\n",
            "existing content must survive (append mode)"
        );
    }

    #[test]
    fn log_tail_is_none_for_missing_or_empty_files() {
        let tmp = TempDir::new().unwrap();
        assert_eq!(read_log_tail(&tmp.path().join("absent.log"), 10), None);

        let empty = tmp.path().join("empty.log");
        std::fs::write(&empty, "\n  \n").unwrap();
        assert_eq!(read_log_tail(&empty, 10), None);
    }
}
