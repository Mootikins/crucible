use anyhow::Result;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Test kiln with temporary directory and database
pub struct TestKiln {
    pub temp_dir: TempDir,
    pub kiln_path: PathBuf,
    pub db_path: PathBuf,
}

impl TestKiln {
    /// Create a new test kiln with temporary directory
    pub fn new() -> Result<Self> {
        let temp_dir = TempDir::new()?;
        let kiln_path = temp_dir.path().join("kiln");
        let db_path = temp_dir.path().join("test.db");

        std::fs::create_dir_all(&kiln_path)?;

        Ok(Self {
            temp_dir,
            kiln_path,
            db_path,
        })
    }
    
    /// Create a single note in the kiln
    pub fn create_note(&self, relative_path: &str, content: &str) -> Result<PathBuf> {
        let full_path = self.kiln_path.join(relative_path);
        
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
    
    /// Get kiln path as string
    pub fn kiln_path_str(&self) -> &str {
        self.kiln_path.to_str().unwrap()
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
    fn test_kiln_creation() {
        let kiln = TestKiln::new().unwrap();
        assert!(kiln.kiln_path.exists());
    }

    #[test]
    fn test_create_note() {
        let kiln = TestKiln::new().unwrap();
        let note_path = kiln.create_note("test.md", "# Test").unwrap();

        assert!(note_path.exists());
        let content = std::fs::read_to_string(&note_path).unwrap();
        assert_eq!(content, "# Test");
    }

    #[test]
    fn test_create_notes_batch() {
        let kiln = TestKiln::new().unwrap();
        kiln.create_notes_batch(vec![
            ("note1.md", "Content 1"),
            ("folder/note2.md", "Content 2"),
        ]).unwrap();
        
        assert!(kiln.kiln_path.join("note1.md").exists());
        assert!(kiln.kiln_path.join("folder/note2.md").exists());
    }
}
