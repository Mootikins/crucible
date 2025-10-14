use anyhow::Result;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Test vault with temporary directory and database
pub struct TestVault {
    pub temp_dir: TempDir,
    pub vault_path: PathBuf,
    pub db_path: PathBuf,
}

impl TestVault {
    /// Create a new test vault with temporary directory
    pub fn new() -> Result<Self> {
        let temp_dir = TempDir::new()?;
        let vault_path = temp_dir.path().join("vault");
        let db_path = temp_dir.path().join("test.db");
        
        std::fs::create_dir_all(&vault_path)?;
        
        Ok(Self {
            temp_dir,
            vault_path,
            db_path,
        })
    }
    
    /// Create a single note in the vault
    pub fn create_note(&self, relative_path: &str, content: &str) -> Result<PathBuf> {
        let full_path = self.vault_path.join(relative_path);
        
        // Create parent directories
        if let Some(parent) = full_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        
        std::fs::write(&full_path, content)?;
        Ok(full_path)
    }
    
    /// Create multiple notes at once
    pub fn create_notes_batch(&self, notes: Vec<(&str, &str)>) -> Result<()> {
        for (path, content) in notes {
            self.create_note(path, content)?;
        }
        Ok(())
    }
    
    /// Get vault path as string
    pub fn vault_path_str(&self) -> &str {
        self.vault_path.to_str().unwrap()
    }
    
    /// Get database path as string
    pub fn db_path_str(&self) -> &str {
        self.db_path.to_str().unwrap()
    }
}

/// Helper function to assert output contains expected string
pub fn assert_output_contains(output: &str, expected: &str) {
    assert!(
        output.contains(expected),
        "Expected output to contain '{}', but got:\n{}",
        expected,
        output
    );
}

/// Helper function to validate JSON output
pub fn assert_json_valid(output: &str) -> serde_json::Value {
    serde_json::from_str(output).expect("Output should be valid JSON")
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_vault_creation() {
        let vault = TestVault::new().unwrap();
        assert!(vault.vault_path.exists());
    }
    
    #[test]
    fn test_create_note() {
        let vault = TestVault::new().unwrap();
        let note_path = vault.create_note("test.md", "# Test").unwrap();
        
        assert!(note_path.exists());
        let content = std::fs::read_to_string(&note_path).unwrap();
        assert_eq!(content, "# Test");
    }
    
    #[test]
    fn test_create_notes_batch() {
        let vault = TestVault::new().unwrap();
        vault.create_notes_batch(vec![
            ("note1.md", "Content 1"),
            ("folder/note2.md", "Content 2"),
        ]).unwrap();
        
        assert!(vault.vault_path.join("note1.md").exists());
        assert!(vault.vault_path.join("folder/note2.md").exists());
    }
}
