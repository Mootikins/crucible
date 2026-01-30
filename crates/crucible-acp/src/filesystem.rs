//! File system operation handlers for agents
//!
//! This module provides safe file system access for agents, with appropriate
//! sandboxing and permission controls.
//!
//! ## Responsibilities
//!
//! - Safe file read/write operations within allowed directories
//! - Path validation and sandboxing
//! - File operation result formatting for agent responses
//!
//! ## Design Principles
//!
//! - **Single Responsibility**: Focused on file system operations
//! - **Security**: All paths are validated against allowed directories
//! - **Dependency Inversion**: Implements traits from crucible-core

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::{ClientError, Result};

/// Configuration for file system access
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSystemConfig {
    /// Root directories that the agent is allowed to access
    pub allowed_roots: Vec<PathBuf>,

    /// Whether to allow write operations
    pub allow_write: bool,

    /// Whether to allow directory creation
    pub allow_create_dirs: bool,

    /// Maximum file size for read operations (in bytes)
    pub max_read_size: usize,
}

impl Default for FileSystemConfig {
    fn default() -> Self {
        Self {
            allowed_roots: vec![],
            allow_write: false,
            allow_create_dirs: false,
            max_read_size: 10 * 1024 * 1024, // 10 MB
        }
    }
}

/// Handles file system operations for agents
///
/// This provides a secure interface for agents to access files within
/// configured allowed directories.
#[derive(Debug)]
pub struct FileSystemHandler {
    config: FileSystemConfig,
}

impl FileSystemHandler {
    /// Create a new file system handler with the given configuration
    ///
    /// # Arguments
    ///
    /// * `config` - File system configuration
    pub fn new(config: FileSystemConfig) -> Self {
        Self { config }
    }

    /// Check if a path is within allowed roots
    ///
    /// # Arguments
    ///
    /// * `path` - The path to check
    ///
    /// # Returns
    ///
    /// `true` if the path is within an allowed root, `false` otherwise
    pub fn is_path_allowed(&self, path: &Path) -> bool {
        // If no allowed roots configured, deny all access
        if self.config.allowed_roots.is_empty() {
            return false;
        }

        // Try to canonicalize the path to resolve any symlinks or .. components
        let canonical_path =
            match path.canonicalize() {
                Ok(p) => p,
                Err(_) => {
                    // If we can't canonicalize (file doesn't exist), we need to resolve
                    // .. components manually to prevent traversal attacks
                    let abs_path = if path.is_absolute() {
                        path.to_path_buf()
                    } else {
                        match std::env::current_dir() {
                            Ok(cwd) => cwd.join(path),
                            Err(_) => return false,
                        }
                    };

                    // Manually resolve path components to handle .. correctly
                    let mut resolved = PathBuf::new();
                    for component in abs_path.components() {
                        match component {
                            std::path::Component::ParentDir => {
                                resolved.pop();
                            }
                            std::path::Component::Normal(part) => {
                                resolved.push(part);
                            }
                            std::path::Component::RootDir => {
                                resolved.push(component);
                            }
                            std::path::Component::CurDir => {
                                // Skip current directory components
                            }
                            std::path::Component::Prefix(_) => {
                                resolved.push(component);
                            }
                        }
                    }

                    // Check if this resolved path would be under any allowed root
                    return self.config.allowed_roots.iter().any(|root| {
                        match root.canonicalize() {
                            Ok(canonical_root) => resolved.starts_with(&canonical_root),
                            Err(_) => resolved.starts_with(root),
                        }
                    });
                }
            };

        // Check if the canonical path is under any of the allowed roots
        self.config
            .allowed_roots
            .iter()
            .any(|root| match root.canonicalize() {
                Ok(canonical_root) => canonical_path.starts_with(&canonical_root),
                Err(_) => false,
            })
    }

    /// Read a file's contents
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the file to read
    ///
    /// # Returns
    ///
    /// The file contents as a string
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The path is not allowed
    /// - The file doesn't exist
    /// - The file is too large
    /// - IO errors occur
    pub async fn read_file(&self, path: &Path) -> Result<String> {
        // Check if path is allowed
        if !self.is_path_allowed(path) {
            return Err(ClientError::PermissionDenied(format!(
                "Access denied to path: {}",
                path.display()
            )));
        }

        // Check if file exists
        if !path.exists() {
            return Err(ClientError::NotFound(format!(
                "File not found: {}",
                path.display()
            )));
        }

        // Check if it's a file (not a directory)
        if !path.is_file() {
            return Err(ClientError::FileSystem(format!(
                "Path is not a file: {}",
                path.display()
            )));
        }

        // Check file size
        let metadata = tokio::fs::metadata(path).await?;
        if metadata.len() > self.config.max_read_size as u64 {
            return Err(ClientError::FileSystem(format!(
                "File too large: {} bytes (max: {})",
                metadata.len(),
                self.config.max_read_size
            )));
        }

        // Read the file
        let content = tokio::fs::read_to_string(path).await?;

        Ok(content)
    }

    /// Write content to a file
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the file to write
    /// * `content` - Content to write
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The path is not allowed
    /// - Write operations are disabled
    /// - IO errors occur
    pub async fn write_file(&self, path: &Path, content: &str) -> Result<()> {
        // Check if write operations are enabled
        if !self.config.allow_write {
            return Err(ClientError::PermissionDenied(
                "Write operations are disabled".to_string(),
            ));
        }

        // Check if path is allowed
        if !self.is_path_allowed(path) {
            return Err(ClientError::PermissionDenied(format!(
                "Access denied to path: {}",
                path.display()
            )));
        }

        // Check if parent directory exists
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                // If directory creation is allowed, create parent directories
                if self.config.allow_create_dirs {
                    tokio::fs::create_dir_all(parent).await?;
                } else {
                    return Err(ClientError::FileSystem(format!(
                        "Parent directory does not exist: {}",
                        parent.display()
                    )));
                }
            }
        }

        // Write the file
        tokio::fs::write(path, content).await?;

        Ok(())
    }

    /// List files in a directory
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the directory
    ///
    /// # Returns
    ///
    /// A list of file paths in the directory
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The path is not allowed
    /// - The directory doesn't exist
    /// - IO errors occur
    pub async fn list_directory(&self, path: &Path) -> Result<Vec<PathBuf>> {
        if !self.is_path_allowed(path) {
            return Err(ClientError::PermissionDenied(format!(
                "Path not allowed: {}",
                path.display()
            )));
        }
        if !path.exists() {
            return Err(ClientError::NotFound(format!(
                "Directory not found: {}",
                path.display()
            )));
        }
        if !path.is_dir() {
            return Err(ClientError::FileSystem(format!(
                "Not a directory: {}",
                path.display()
            )));
        }
        let mut entries = Vec::new();
        let mut read_dir = tokio::fs::read_dir(path).await.map_err(ClientError::Io)?;
        while let Some(entry) = read_dir.next_entry().await.map_err(ClientError::Io)? {
            entries.push(entry.path());
        }
        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_handler_creation() {
        let config = FileSystemConfig::default();
        let handler = FileSystemHandler::new(config);
        // With empty allowed_roots, nothing should be allowed
        assert!(!handler.is_path_allowed(Path::new("some_path")));
    }

    #[test]
    fn test_default_config() {
        let config = FileSystemConfig::default();
        assert!(config.allowed_roots.is_empty());
        assert!(!config.allow_write);
        assert!(!config.allow_create_dirs);
        assert_eq!(config.max_read_size, 10 * 1024 * 1024);
    }

    #[test]
    fn test_path_validation() {
        let temp_dir = TempDir::new().unwrap();
        let allowed_root = temp_dir.path().canonicalize().unwrap();

        let config = FileSystemConfig {
            allowed_roots: vec![allowed_root.clone()],
            allow_write: false,
            allow_create_dirs: false,
            max_read_size: 10 * 1024 * 1024,
        };
        let handler = FileSystemHandler::new(config);

        // Path within allowed root should be allowed
        let allowed_path = allowed_root.join("test.txt");
        assert!(handler.is_path_allowed(&allowed_path));

        // Path outside allowed root should not be allowed
        let outside_dir = TempDir::new().unwrap();
        let disallowed_path = outside_dir.path().join("test.txt");
        assert!(!handler.is_path_allowed(&disallowed_path));

        // Parent directory traversal should not be allowed
        let traversal_path = allowed_root.join("../outside.txt");
        assert!(!handler.is_path_allowed(&traversal_path));
    }

    #[tokio::test]
    async fn test_read_file() {
        let temp_dir = TempDir::new().unwrap();
        let allowed_root = temp_dir.path().canonicalize().unwrap();

        // Create a test file
        let test_file = allowed_root.join("test.txt");
        let test_content = "Hello, World!";
        fs::write(&test_file, test_content).unwrap();

        let config = FileSystemConfig {
            allowed_roots: vec![allowed_root],
            allow_write: false,
            allow_create_dirs: false,
            max_read_size: 10 * 1024 * 1024,
        };
        let handler = FileSystemHandler::new(config);

        // Should successfully read the file
        let result = handler.read_file(&test_file).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), test_content);
    }

    #[tokio::test]
    async fn test_read_file_permission_denied() {
        let temp_dir = TempDir::new().unwrap();
        let allowed_root = temp_dir.path().canonicalize().unwrap();

        let config = FileSystemConfig {
            allowed_roots: vec![allowed_root],
            allow_write: false,
            allow_create_dirs: false,
            max_read_size: 10 * 1024 * 1024,
        };
        let handler = FileSystemHandler::new(config);

        // Try to read a file outside allowed root
        let outside_dir = TempDir::new().unwrap();
        let disallowed_path = outside_dir.path().join("test.txt");
        // Create the file so it exists (otherwise we might get NotFound instead of PermissionDenied)
        fs::write(&disallowed_path, "secret").unwrap();

        let result = handler.read_file(&disallowed_path).await;
        assert!(result.is_err());

        match result {
            Err(ClientError::PermissionDenied(_)) => {}
            _ => panic!("Expected PermissionDenied error"),
        }
    }

    #[tokio::test]
    async fn test_read_file_size_limit() {
        let temp_dir = TempDir::new().unwrap();
        let allowed_root = temp_dir.path().canonicalize().unwrap();

        // Create a file that exceeds the limit
        let test_file = allowed_root.join("large.txt");
        let large_content = "x".repeat(100); // 100 bytes
        fs::write(&test_file, &large_content).unwrap();

        let config = FileSystemConfig {
            allowed_roots: vec![allowed_root],
            allow_write: false,
            allow_create_dirs: false,
            max_read_size: 50, // Only allow 50 bytes
        };
        let handler = FileSystemHandler::new(config);

        // Should fail due to size limit
        let result = handler.read_file(&test_file).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_read_nonexistent_file() {
        let temp_dir = TempDir::new().unwrap();
        let allowed_root = temp_dir.path().canonicalize().unwrap();

        let config = FileSystemConfig {
            allowed_roots: vec![allowed_root.clone()],
            allow_write: false,
            allow_create_dirs: false,
            max_read_size: 10 * 1024 * 1024,
        };
        let handler = FileSystemHandler::new(config);

        // Try to read a non-existent file
        let missing_file = allowed_root.join("doesnotexist.txt");
        let result = handler.read_file(&missing_file).await;
        assert!(result.is_err());

        match result {
            Err(ClientError::NotFound(_)) => {}
            _ => panic!("Expected NotFound error"),
        }
    }

    #[tokio::test]
    async fn test_write_file() {
        let temp_dir = TempDir::new().unwrap();
        let allowed_root = temp_dir.path().canonicalize().unwrap();

        let config = FileSystemConfig {
            allowed_roots: vec![allowed_root.clone()],
            allow_write: true,
            allow_create_dirs: false,
            max_read_size: 10 * 1024 * 1024,
        };
        let handler = FileSystemHandler::new(config);

        // Should be able to write a file
        let test_file = allowed_root.join("output.txt");
        let test_content = "Written content";
        let result = handler.write_file(&test_file, test_content).await;
        assert!(result.is_ok());

        // Verify the file was actually written
        let read_content = fs::read_to_string(&test_file).unwrap();
        assert_eq!(read_content, test_content);
    }

    #[tokio::test]
    async fn test_write_file_disabled() {
        let temp_dir = TempDir::new().unwrap();
        let allowed_root = temp_dir.path().canonicalize().unwrap();

        let config = FileSystemConfig {
            allowed_roots: vec![allowed_root.clone()],
            allow_write: false, // Write disabled
            allow_create_dirs: false,
            max_read_size: 10 * 1024 * 1024,
        };
        let handler = FileSystemHandler::new(config);

        // Should fail due to write being disabled
        let test_file = allowed_root.join("output.txt");
        let result = handler.write_file(&test_file, "content").await;
        assert!(result.is_err());

        match result {
            Err(ClientError::PermissionDenied(_)) => {}
            _ => panic!("Expected PermissionDenied error"),
        }
    }

    #[tokio::test]
    async fn test_write_file_permission_denied() {
        let temp_dir = TempDir::new().unwrap();
        let allowed_root = temp_dir.path().canonicalize().unwrap();

        let config = FileSystemConfig {
            allowed_roots: vec![allowed_root],
            allow_write: true,
            allow_create_dirs: false,
            max_read_size: 10 * 1024 * 1024,
        };
        let handler = FileSystemHandler::new(config);

        // Try to write outside allowed root
        let outside_dir = TempDir::new().unwrap();
        let disallowed_path = outside_dir.path().join("test.txt");
        let result = handler.write_file(&disallowed_path, "content").await;
        assert!(result.is_err());

        match result {
            Err(ClientError::PermissionDenied(_)) => {}
            _ => panic!("Expected PermissionDenied error"),
        }
    }

    #[tokio::test]
    async fn test_write_file_create_dirs() {
        let temp_dir = TempDir::new().unwrap();
        let allowed_root = temp_dir.path().canonicalize().unwrap();

        let config = FileSystemConfig {
            allowed_roots: vec![allowed_root.clone()],
            allow_write: true,
            allow_create_dirs: true,
            max_read_size: 10 * 1024 * 1024,
        };
        let handler = FileSystemHandler::new(config);

        // Should be able to write to a nested path, creating directories
        let nested_file = allowed_root.join("dir1").join("dir2").join("file.txt");
        let result = handler.write_file(&nested_file, "content").await;
        assert!(result.is_ok());

        // Verify the directories were created
        assert!(nested_file.parent().unwrap().exists());
        assert!(nested_file.exists());
    }

    #[tokio::test]
    async fn test_write_file_no_create_dirs() {
        let temp_dir = TempDir::new().unwrap();
        let allowed_root = temp_dir.path().canonicalize().unwrap();

        let config = FileSystemConfig {
            allowed_roots: vec![allowed_root.clone()],
            allow_write: true,
            allow_create_dirs: false,
            max_read_size: 10 * 1024 * 1024,
        };
        let handler = FileSystemHandler::new(config);

        // Should fail because parent directory doesn't exist
        let nested_file = allowed_root.join("nonexistent").join("file.txt");
        let result = handler.write_file(&nested_file, "content").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_list_directory_success() {
        let temp_dir = TempDir::new().unwrap();
        let allowed_root = temp_dir.path().canonicalize().unwrap();

        // Create 2 files and 1 subdirectory
        fs::write(allowed_root.join("file1.txt"), "content1").unwrap();
        fs::write(allowed_root.join("file2.txt"), "content2").unwrap();
        fs::create_dir(allowed_root.join("subdir")).unwrap();

        let config = FileSystemConfig {
            allowed_roots: vec![allowed_root.clone()],
            allow_write: false,
            allow_create_dirs: false,
            max_read_size: 10 * 1024 * 1024,
        };
        let handler = FileSystemHandler::new(config);

        let result = handler.list_directory(&allowed_root).await;
        assert!(result.is_ok());
        let entries = result.unwrap();
        assert_eq!(entries.len(), 3);
    }

    #[tokio::test]
    async fn test_list_directory_permission_denied() {
        let temp_dir = TempDir::new().unwrap();
        let allowed_root = temp_dir.path().canonicalize().unwrap();

        let config = FileSystemConfig {
            allowed_roots: vec![allowed_root],
            allow_write: false,
            allow_create_dirs: false,
            max_read_size: 10 * 1024 * 1024,
        };
        let handler = FileSystemHandler::new(config);

        // Try to list a directory outside allowed root
        let outside_dir = TempDir::new().unwrap();
        let result = handler.list_directory(outside_dir.path()).await;
        assert!(result.is_err());

        match result {
            Err(ClientError::PermissionDenied(_)) => {}
            _ => panic!("Expected PermissionDenied error"),
        }
    }

    #[tokio::test]
    async fn test_list_directory_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let allowed_root = temp_dir.path().canonicalize().unwrap();

        let config = FileSystemConfig {
            allowed_roots: vec![allowed_root.clone()],
            allow_write: false,
            allow_create_dirs: false,
            max_read_size: 10 * 1024 * 1024,
        };
        let handler = FileSystemHandler::new(config);

        // Try to list a non-existent directory
        let missing_dir = allowed_root.join("doesnotexist");
        let result = handler.list_directory(&missing_dir).await;
        assert!(result.is_err());

        match result {
            Err(ClientError::NotFound(_)) => {}
            _ => panic!("Expected NotFound error"),
        }
    }

    #[tokio::test]
    async fn test_list_directory_not_a_directory() {
        let temp_dir = TempDir::new().unwrap();
        let allowed_root = temp_dir.path().canonicalize().unwrap();

        // Create a regular file
        let test_file = allowed_root.join("file.txt");
        fs::write(&test_file, "content").unwrap();

        let config = FileSystemConfig {
            allowed_roots: vec![allowed_root],
            allow_write: false,
            allow_create_dirs: false,
            max_read_size: 10 * 1024 * 1024,
        };
        let handler = FileSystemHandler::new(config);

        // Try to list a regular file as if it were a directory
        let result = handler.list_directory(&test_file).await;
        assert!(result.is_err());

        match result {
            Err(ClientError::FileSystem(_)) => {}
            _ => panic!("Expected FileSystem error"),
        }
    }

    #[tokio::test]
    async fn test_list_directory_empty() {
        let temp_dir = TempDir::new().unwrap();
        let allowed_root = temp_dir.path().canonicalize().unwrap();

        let config = FileSystemConfig {
            allowed_roots: vec![allowed_root.clone()],
            allow_write: false,
            allow_create_dirs: false,
            max_read_size: 10 * 1024 * 1024,
        };
        let handler = FileSystemHandler::new(config);

        // List an empty directory
        let result = handler.list_directory(&allowed_root).await;
        assert!(result.is_ok());
        let entries = result.unwrap();
        assert_eq!(entries.len(), 0);
    }
}
