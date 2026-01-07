//! Daemon lifecycle management for single-binary pattern
//!
//! This module handles forking the `cru` binary as a db-server and
//! ensuring it's running before connecting.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use tracing::{debug, info, warn};

/// Get the default socket path for db-server
///
/// Uses XDG_RUNTIME_DIR if available, otherwise falls back to /tmp.
/// This matches the default path used by `cru db-server`.
pub fn default_socket_path() -> PathBuf {
    if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        PathBuf::from(runtime_dir).join("crucible-db.sock")
    } else {
        PathBuf::from("/tmp/crucible-db.sock")
    }
}

/// Check if daemon is running (socket exists and accepts connections)
///
/// Unlike the PID-based check in crucible-daemon, this verifies the socket
/// is actually connectable, which is more reliable for our use case.
pub fn is_daemon_running(socket: &Path) -> bool {
    if !socket.exists() {
        return false;
    }

    // Try to connect to verify the socket is alive
    match std::os::unix::net::UnixStream::connect(socket) {
        Ok(_) => true,
        Err(e) => {
            debug!("Socket exists but connection failed: {}", e);
            false
        }
    }
}

/// Fork self as db-server daemon
///
/// Spawns the current executable with `db-server` subcommand.
/// The spawned process is detached and runs in the background.
pub fn fork_daemon(socket: &Path, idle_timeout: u64) -> Result<()> {
    let exe = std::env::current_exe().context("Failed to get current executable path")?;

    debug!(
        "Forking db-server: {} db-server --socket {:?} --idle-timeout {}",
        exe.display(),
        socket,
        idle_timeout
    );

    Command::new(&exe)
        .arg("--log-level")
        .arg("info") // Enable file logging to ~/.crucible/db-server.log
        .arg("db-server")
        .arg("--socket")
        .arg(socket)
        .arg("--idle-timeout")
        .arg(idle_timeout.to_string())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .context("Failed to spawn db-server process")?;

    info!("Forked db-server process");
    Ok(())
}

/// Ensure daemon is running, forking if needed
///
/// If the daemon is already running, returns immediately.
/// Otherwise, forks a new daemon and waits for the socket with exponential backoff.
pub async fn ensure_daemon(socket: &Path, idle_timeout: u64) -> Result<()> {
    if is_daemon_running(socket) {
        debug!("Daemon already running at {:?}", socket);
        return Ok(());
    }

    info!("Daemon not running, forking new instance");
    fork_daemon(socket, idle_timeout)?;

    // Wait for socket with exponential backoff
    let mut delay = Duration::from_millis(50);
    let max_attempts = 10;

    for attempt in 0..max_attempts {
        tokio::time::sleep(delay).await;

        if is_daemon_running(socket) {
            info!("Daemon ready after {} attempts", attempt + 1);
            return Ok(());
        }

        delay = std::cmp::min(delay * 2, Duration::from_secs(1));

        if attempt > 5 {
            warn!(
                "Daemon not ready after {} attempts, retrying...",
                attempt + 1
            );
        }
    }

    anyhow::bail!(
        "Failed to start db-server daemon after {} attempts. \
         Check logs at ~/.crucible/db-server.log",
        max_attempts
    )
}

/// Check if a database lock file is held by another process
///
/// RocksDB uses fcntl (POSIX) locks, not flock (BSD) locks. This function
/// uses F_GETLK to check if another process holds the lock without actually
/// acquiring it.
#[cfg(unix)]
pub fn is_db_locked(db_path: &Path) -> bool {
    use std::fs::OpenOptions;
    use std::os::unix::io::AsRawFd;

    let lock_path = db_path.join("LOCK");

    if !lock_path.exists() {
        return false;
    }

    // Try to open the lock file
    match OpenOptions::new().read(true).write(true).open(&lock_path) {
        Ok(file) => {
            let fd = file.as_raw_fd();

            // Use F_GETLK to check if file is locked without acquiring
            // This is the same mechanism RocksDB uses
            let mut lock = libc::flock {
                l_type: libc::F_WRLCK as i16, // Check for write lock
                l_whence: libc::SEEK_SET as i16,
                l_start: 0,
                l_len: 0, // Entire file
                l_pid: 0,
            };

            let result = unsafe { libc::fcntl(fd, libc::F_GETLK, &mut lock) };

            if result == -1 {
                debug!("fcntl F_GETLK failed: {}", std::io::Error::last_os_error());
                return false;
            }

            if lock.l_type == libc::F_UNLCK as i16 {
                // No lock held
                false
            } else {
                // Lock is held by another process
                debug!(
                    "Database lock held by process {}: {:?}",
                    lock.l_pid, lock_path
                );
                true
            }
        }
        Err(e) => {
            debug!("Failed to check lock file {:?}: {}", lock_path, e);
            false // Can't determine, assume not locked
        }
    }
}

#[cfg(not(unix))]
pub fn is_db_locked(_db_path: &Path) -> bool {
    // On non-Unix, we can't easily check file locks
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_default_socket_path_ends_with_sock() {
        let path = default_socket_path();
        assert!(path.ends_with("crucible-db.sock"));
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

        // Create a regular file (not a socket)
        std::fs::write(&socket, "not a socket").unwrap();

        // Should return false because it's not a real socket
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

        // Create a LOCK file (simulating RocksDB)
        std::fs::write(db_path.join("LOCK"), "").unwrap();

        // Lock file exists but not held by fcntl
        assert!(!is_db_locked(db_path));
    }

    /// Test lock detection using a child process
    ///
    /// fcntl (POSIX) locks are per-process, not per-fd, so we need a separate
    /// process to hold the lock for detection to work.
    #[cfg(unix)]
    #[test]
    fn test_is_db_locked_true_when_lock_held_by_another_process() {
        use std::io::{BufRead, BufReader, Write};
        use std::process::{Command, Stdio};

        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path();
        let lock_path = db_path.join("LOCK");

        // Create LOCK file
        std::fs::write(&lock_path, "").unwrap();

        // Spawn child process that holds the lock
        let mut child = Command::new("python3")
            .arg("-c")
            .arg(format!(
                r#"
import fcntl
import sys

fd = open("{}", "r+")
lock = fcntl.flock(fd.fileno(), fcntl.LOCK_EX)  # This actually uses flock, not fcntl
# But we need fcntl lock, so use lockf instead
fcntl.lockf(fd.fileno(), fcntl.LOCK_EX)
print("LOCKED", flush=True)
sys.stdin.readline()  # Wait for signal to release
"#,
                lock_path.display()
            ))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("Failed to spawn child process");

        // Wait for child to acquire lock
        let stdout = child.stdout.take().unwrap();
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        assert!(line.contains("LOCKED"), "Child should acquire lock");

        // Now check if we detect the lock
        assert!(
            is_db_locked(db_path),
            "Should detect lock held by another process"
        );

        // Signal child to release and exit
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(b"\n");
        }
        let _ = child.wait();
    }

    // Note: We don't test fork_daemon or ensure_daemon here because they require
    // actually running the binary. Those are tested in integration tests.
}
