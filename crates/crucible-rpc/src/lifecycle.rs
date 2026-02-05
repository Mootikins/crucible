//! Daemon lifecycle utilities
//!
//! This module provides utilities for checking daemon status and database locks.
//! The actual daemon spawning is handled by `DaemonClient::start_daemon()`.

use std::path::{Path, PathBuf};
use tracing::debug;

pub fn default_socket_path() -> PathBuf {
    crucible_protocol::socket_path()
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

/// Check if a database lock file is held by another process
///
/// RocksDB uses fcntl (POSIX) locks. This function uses F_GETLK to check
/// if another process holds the lock without actually acquiring it.
#[cfg(unix)]
pub fn is_db_locked(db_path: &Path) -> bool {
    use std::fs::OpenOptions;
    use std::os::unix::io::AsRawFd;

    let lock_path = db_path.join("LOCK");

    if !lock_path.exists() {
        return false;
    }

    match OpenOptions::new().read(true).write(true).open(&lock_path) {
        Ok(file) => {
            let fd = file.as_raw_fd();

            let mut lock = libc::flock {
                l_type: libc::F_WRLCK as i16,
                l_whence: libc::SEEK_SET as i16,
                l_start: 0,
                l_len: 0,
                l_pid: 0,
            };

            let result = unsafe { libc::fcntl(fd, libc::F_GETLK, &mut lock) };

            if result == -1 {
                debug!("fcntl F_GETLK failed: {}", std::io::Error::last_os_error());
                return false;
            }

            if lock.l_type == libc::F_UNLCK as i16 {
                false
            } else {
                debug!(
                    "Database lock held by process {}: {:?}",
                    lock.l_pid, lock_path
                );
                true
            }
        }
        Err(e) => {
            debug!("Failed to check lock file {:?}: {}", lock_path, e);
            false
        }
    }
}

#[cfg(not(unix))]
pub fn is_db_locked(_db_path: &Path) -> bool {
    false
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
    fn test_is_db_locked_false_when_no_lock_file() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("nonexistent.db");
        assert!(!is_db_locked(&db_path));
    }

    #[test]
    fn test_is_db_locked_false_when_lock_exists_but_not_held() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path();
        std::fs::write(db_path.join("LOCK"), "").unwrap();
        assert!(!is_db_locked(db_path));
    }

    #[cfg(unix)]
    #[test]
    fn test_is_db_locked_true_when_lock_held_by_another_process() {
        use std::io::{BufRead, BufReader, Write};
        use std::process::{Command, Stdio};

        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path();
        let lock_path = db_path.join("LOCK");

        std::fs::write(&lock_path, "").unwrap();

        let mut child = Command::new("python3")
            .arg("-c")
            .arg(format!(
                r#"
import fcntl
import sys

fd = open("{}", "r+")
fcntl.lockf(fd.fileno(), fcntl.LOCK_EX)
print("LOCKED", flush=True)
sys.stdin.readline()
"#,
                lock_path.display()
            ))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("Failed to spawn child process");

        let stdout = child.stdout.take().unwrap();
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        assert!(line.contains("LOCKED"), "Child should acquire lock");

        assert!(
            is_db_locked(db_path),
            "Should detect lock held by another process"
        );

        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(b"\n");
        }
        let _ = child.wait();
    }
}
