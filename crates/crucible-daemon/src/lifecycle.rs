//! Daemon lifecycle: paths, PID file, status checks

use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};

/// Get the socket path for the daemon
pub fn socket_path() -> PathBuf {
    if let Ok(path) = std::env::var("CRUCIBLE_DAEMON_SOCKET") {
        return PathBuf::from(path);
    }
    dirs::runtime_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("crucible")
        .join("daemon.sock")
}

/// Get the PID file path
pub fn pid_path() -> PathBuf {
    if let Ok(path) = std::env::var("CRUCIBLE_DAEMON_PID") {
        return PathBuf::from(path);
    }
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("~/.config"))
        .join("crucible")
        .join("daemon.pid")
}

/// Write PID file
pub fn write_pid_file(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, std::process::id().to_string())?;
    Ok(())
}

/// Remove PID file
pub fn remove_pid_file(path: &Path) {
    let _ = fs::remove_file(path);
}

/// Check if daemon is running by checking PID file and /proc
pub fn is_daemon_running() -> bool {
    let pid_file = pid_path();
    is_daemon_running_at(&pid_file)
}

/// Check if daemon is running at specific PID file path
pub fn is_daemon_running_at(path: &Path) -> bool {
    if let Ok(contents) = fs::read_to_string(path) {
        if let Ok(pid) = contents.trim().parse::<u32>() {
            // Check if process exists (Linux-specific)
            return Path::new(&format!("/proc/{}", pid)).exists();
        }
    }
    false
}

/// Remove socket file if it exists
pub fn remove_socket(path: &Path) {
    let _ = fs::remove_file(path);
}

/// Wait for SIGTERM or SIGINT signal
pub async fn wait_for_shutdown() -> Result<()> {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};

        let mut sigterm = signal(SignalKind::terminate())?;
        let mut sigint = signal(SignalKind::interrupt())?;

        tokio::select! {
            _ = sigterm.recv() => {
                tracing::info!("Received SIGTERM");
            }
            _ = sigint.recv() => {
                tracing::info!("Received SIGINT");
            }
        }
    }

    #[cfg(not(unix))]
    {
        // On non-Unix systems, just wait for Ctrl+C
        tokio::signal::ctrl_c().await?;
        tracing::info!("Received shutdown signal");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_socket_path_not_empty() {
        let path = socket_path();
        assert!(path.to_string_lossy().contains("crucible"));
    }

    #[test]
    fn test_pid_file_lifecycle() {
        let tmp = TempDir::new().unwrap();
        let pid_file = tmp.path().join("test.pid");

        write_pid_file(&pid_file).unwrap();
        assert!(pid_file.exists());

        let contents = fs::read_to_string(&pid_file).unwrap();
        assert_eq!(contents, std::process::id().to_string());

        remove_pid_file(&pid_file);
        assert!(!pid_file.exists());
    }

    #[test]
    fn test_is_daemon_running_current_process() {
        let tmp = TempDir::new().unwrap();
        let pid_file = tmp.path().join("test.pid");

        // Write our own PID
        write_pid_file(&pid_file).unwrap();

        // Should detect ourselves as running
        assert!(is_daemon_running_at(&pid_file));

        remove_pid_file(&pid_file);
    }

    #[test]
    fn test_is_daemon_running_nonexistent() {
        let tmp = TempDir::new().unwrap();
        let pid_file = tmp.path().join("nonexistent.pid");

        // Should return false for nonexistent file
        assert!(!is_daemon_running_at(&pid_file));
    }

    #[test]
    fn test_is_daemon_running_dead_process() {
        let tmp = TempDir::new().unwrap();
        let pid_file = tmp.path().join("test.pid");

        // Write a PID that definitely doesn't exist (very high number)
        fs::write(&pid_file, "999999").unwrap();

        // Should return false since process doesn't exist
        assert!(!is_daemon_running_at(&pid_file));

        remove_pid_file(&pid_file);
    }

    #[test]
    fn test_remove_socket() {
        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");

        // Create a dummy socket file
        fs::write(&sock_path, "").unwrap();
        assert!(sock_path.exists());

        remove_socket(&sock_path);
        assert!(!sock_path.exists());

        // Should not panic on nonexistent file
        remove_socket(&sock_path);
    }
}
