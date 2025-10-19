// Command history management
//
// Provides persistent command history with deduplication

use anyhow::Result;
use reedline::{FileBackedHistory, History};
use std::path::PathBuf;

/// Command history manager
pub struct CommandHistory {
    /// Reedline history backend
    history: Box<dyn History>,

    /// History file path
    file_path: PathBuf,
}

impl CommandHistory {
    /// Create new history manager
    pub fn new(file_path: PathBuf) -> Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let history = Box::new(
            FileBackedHistory::with_file(
                10_000, // Max entries
                file_path.clone(),
            )
            .map_err(|e| anyhow::anyhow!("Failed to create history: {}", e))?
        );

        Ok(Self { history, file_path })
    }

    /// Add command to history (skips empty and duplicates)
    pub fn add(&mut self, command: &str) {
        // Skip empty lines
        if command.trim().is_empty() {
            return;
        }

        // Skip if same as last command (like bash HISTCONTROL=ignoredups)
        // Use search to get the last command
        let filter = reedline::SearchFilter::anything(None);
        let query = reedline::SearchQuery::last_with_search(filter);

        if let Ok(items) = self.history.search(query) {
            if let Some(last) = items.first() {
                if last.command_line == command {
                    return;
                }
            }
        }

        // Add to history
        let _ = self.history.save(reedline::HistoryItem {
            command_line: command.to_string(),
            id: None,
            start_timestamp: None,
            session_id: None,
            hostname: None,
            cwd: None,
            duration: None,
            exit_status: None,
            more_info: None,
        });
    }

    /// Search history (fuzzy match)
    pub fn search(&self, pattern: &str) -> Vec<String> {
        // Use search API to find commands containing the pattern
        let filter = reedline::SearchFilter::from_text_search(
            reedline::CommandLineSearch::Substring(pattern.into()),
            None,
        );
        let mut query = reedline::SearchQuery::last_with_search(filter);
        query.limit = Some(1000);

        self.history
            .search(query)
            .unwrap_or_default()
            .into_iter()
            .map(|item| item.command_line)
            .collect()
    }

    /// Get last N commands
    pub fn get_last_n(&self, n: usize) -> Vec<String> {
        let filter = reedline::SearchFilter::anything(None);
        let mut query = reedline::SearchQuery::last_with_search(filter);
        query.limit = Some(n as i64);

        self.history
            .search(query)
            .unwrap_or_default()
            .into_iter()
            .rev() // Reverse to get oldest first
            .map(|item| item.command_line)
            .collect()
    }

    /// Get total history size
    pub fn len(&self) -> usize {
        self.history.count(reedline::SearchQuery::everything(
            reedline::SearchDirection::Backward,
            None,
        )).unwrap_or(0) as usize
    }

    /// Check if history is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Clear all history
    pub fn clear(&mut self) -> Result<()> {
        // Use the History trait's clear method
        self.history.clear()
            .map_err(|e| anyhow::anyhow!("Failed to clear history: {}", e))?;

        // Manually remove the file since FileBackedHistory might leave an empty file
        if self.file_path.exists() {
            std::fs::remove_file(&self.file_path)?;
        }

        // Recreate history backend
        self.history = Box::new(
            FileBackedHistory::with_file(10_000, self.file_path.clone())
                .map_err(|e| anyhow::anyhow!("Failed to recreate history: {}", e))?
        );

        Ok(())
    }

    /// Clone history backend for reedline
    pub fn clone_backend(&self) -> Box<dyn History> {
        // Create a new instance pointing to the same file
        Box::new(
            FileBackedHistory::with_file(10_000, self.file_path.clone())
                .expect("Failed to clone history backend")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_history_creation() {
        let dir = tempdir().unwrap();
        let history_path = dir.path().join("test_history");

        let history = CommandHistory::new(history_path.clone());
        assert!(history.is_ok());
        assert!(history_path.exists());
    }

    #[test]
    fn test_add_commands() {
        let dir = tempdir().unwrap();
        let history_path = dir.path().join("test_history");
        let mut history = CommandHistory::new(history_path).unwrap();

        history.add("SELECT * FROM notes");
        history.add(":tools");
        history.add("SELECT * FROM tags");

        assert_eq!(history.len(), 3);
    }

    #[test]
    fn test_duplicate_prevention() {
        let dir = tempdir().unwrap();
        let history_path = dir.path().join("test_history");
        let mut history = CommandHistory::new(history_path).unwrap();

        history.add("SELECT * FROM notes");
        history.add("SELECT * FROM notes"); // Duplicate, should be skipped

        assert_eq!(history.len(), 1);
    }

    #[test]
    fn test_empty_line_skipped() {
        let dir = tempdir().unwrap();
        let history_path = dir.path().join("test_history");
        let mut history = CommandHistory::new(history_path).unwrap();

        history.add("");
        history.add("   ");
        history.add("\n");

        assert_eq!(history.len(), 0);
    }

    #[test]
    fn test_get_last_n() {
        let dir = tempdir().unwrap();
        let history_path = dir.path().join("test_history");
        let mut history = CommandHistory::new(history_path).unwrap();

        history.add("cmd1");
        history.add("cmd2");
        history.add("cmd3");
        history.add("cmd4");
        history.add("cmd5");

        let last_3 = history.get_last_n(3);
        assert_eq!(last_3.len(), 3);
        assert_eq!(last_3[0], "cmd3");
        assert_eq!(last_3[1], "cmd4");
        assert_eq!(last_3[2], "cmd5");
    }

    #[test]
    fn test_search() {
        let dir = tempdir().unwrap();
        let history_path = dir.path().join("test_history");
        let mut history = CommandHistory::new(history_path).unwrap();

        history.add("SELECT * FROM notes");
        history.add(":tools");
        history.add("SELECT * FROM tags WHERE name = 'test'");
        history.add(":stats");

        let results = history.search("SELECT");
        assert_eq!(results.len(), 2);

        let results = history.search(":tools");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_clear() {
        let dir = tempdir().unwrap();
        let history_path = dir.path().join("test_history");
        let mut history = CommandHistory::new(history_path.clone()).unwrap();

        history.add("cmd1");
        history.add("cmd2");
        assert_eq!(history.len(), 2);

        history.clear().unwrap();
        assert_eq!(history.len(), 0);

        // Note: FileBackedHistory recreates the file when initialized,
        // so we check that history is empty rather than file doesn't exist
        assert!(history.is_empty());
    }
}
