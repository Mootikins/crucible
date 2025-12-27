//! File operations for inbox

use std::path::{Path, PathBuf};
use std::{env, fs};

use thiserror::Error;

use crate::{parse, Inbox};
#[cfg(not(target_arch = "wasm32"))]
use crate::render;

#[derive(Debug, Error)]
pub enum FileError {
    #[error("ZELLIJ_SESSION_NAME not set and no --file provided")]
    NoSession,
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Get the inbox file path for the current session
#[cfg(not(target_arch = "wasm32"))]
pub fn inbox_path() -> Result<PathBuf, FileError> {
    // Check override first
    if let Ok(path) = env::var("ZELLIJ_INBOX_FILE") {
        return Ok(PathBuf::from(path));
    }

    // Get session name
    let session = env::var("ZELLIJ_SESSION_NAME").map_err(|_| FileError::NoSession)?;

    // Build path
    let base = dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("zellij-inbox");

    Ok(base.join(format!("{}.md", session)))
}

/// Build inbox path from session name (for WASM)
#[cfg(target_arch = "wasm32")]
pub fn inbox_path_for_session(session: &str) -> PathBuf {
    // In WASM we can't use dirs crate, check XDG_DATA_HOME then fall back
    let data_dir = env::var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            // Fall back to ~/.local/share (XDG default)
            env::var("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("/home").join(
                    env::var("USER").unwrap_or_else(|_| "user".to_string())
                ))
                .join(".local/share")
        });

    data_dir.join("zellij-inbox").join(format!("{}.md", session))
}

/// Load inbox from file (returns empty inbox if file doesn't exist)
pub fn load(path: &Path) -> Result<Inbox, FileError> {
    match fs::read_to_string(path) {
        Ok(content) => Ok(parse::parse(&content)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Inbox::new()),
        Err(e) => Err(FileError::Io(e)),
    }
}

/// Save inbox to file (creates parent dirs, deletes file if empty)
#[cfg(not(target_arch = "wasm32"))]
pub fn save(path: &Path, inbox: &Inbox) -> Result<(), FileError> {
    if inbox.is_empty() {
        // Delete file if it exists
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(FileError::Io(e)),
        }
    } else {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = render::render(inbox);
        fs::write(path, content)?;
        Ok(())
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::{InboxItem, Status};
    use tempfile::TempDir;

    #[test]
    fn load_nonexistent_returns_empty() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nonexistent.md");
        let inbox = load(&path).unwrap();
        assert!(inbox.is_empty());
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.md");

        let mut inbox = Inbox::new();
        inbox.upsert(InboxItem {
            text: "claude-code: Test".to_string(),
            pane_id: 42,
            project: "test-project".to_string(),
            status: Status::Waiting,
        });

        save(&path, &inbox).unwrap();
        assert!(path.exists());

        let loaded = load(&path).unwrap();
        assert_eq!(loaded.items.len(), 1);
        assert_eq!(loaded.items[0].pane_id, 42);
    }

    #[test]
    fn save_empty_deletes_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.md");

        // Create file first
        fs::write(&path, "test").unwrap();
        assert!(path.exists());

        // Save empty inbox
        let inbox = Inbox::new();
        save(&path, &inbox).unwrap();
        assert!(!path.exists());
    }
}
