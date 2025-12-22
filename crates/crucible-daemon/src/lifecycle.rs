//! Daemon lifecycle: paths, PID file, status checks

use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};

/// Get the socket path for the daemon
pub fn socket_path() -> PathBuf {
    dirs::runtime_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("crucible")
        .join("daemon.sock")
}

/// Get the PID file path
pub fn pid_path() -> PathBuf {
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
}
