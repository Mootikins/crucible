use std::fs;
use std::path::{Path, PathBuf};

/// Get the socket path for the daemon
///
/// Priority:
/// 1. `CRUCIBLE_SOCKET` environment variable (if set)
/// 2. `$XDG_RUNTIME_DIR/crucible.sock` (if XDG_RUNTIME_DIR is set)
/// 3. `/tmp/crucible.sock` (fallback)
pub fn socket_path() -> PathBuf {
    if let Ok(path) = std::env::var("CRUCIBLE_SOCKET") {
        return PathBuf::from(path);
    }
    dirs::runtime_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("crucible.sock")
}

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
        assert!(path.to_string_lossy().contains("crucible.sock"));
    }

    #[test]
    fn test_remove_socket() {
        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");
        fs::write(&sock_path, "").unwrap();
        assert!(sock_path.exists());
        remove_socket(&sock_path);
        assert!(!sock_path.exists());
        remove_socket(&sock_path); // Should not panic
    }
}
