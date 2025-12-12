//! Completion sources for the chat TUI
//!
//! This module provides the `CompletionSource` trait and implementations
//! to convert various data sources into completion items for fuzzy completion.
//!
//! ## Architecture
//!
//! The `CompletionSource` trait enables:
//! - **Dependency Inversion**: ChatApp depends on the trait, not concrete implementations
//! - **Open/Closed**: New sources can be added without modifying ChatApp
//! - **Hot-reload**: Sources can be swapped at runtime (e.g., when ACP publishes commands)

use std::path::PathBuf;

use crate::chat::slash_registry::SlashCommandRegistry;
use super::completion::{CompletionItem, CompletionType};

/// Trait for completion data sources
///
/// Implement this to provide completion items from various sources
/// (slash commands, files, agents, etc.)
///
/// ## Example
///
/// ```ignore
/// struct MySource { items: Vec<CompletionItem> }
///
/// impl CompletionSource for MySource {
///     fn get_items(&self) -> Vec<CompletionItem> {
///         self.items.clone()
///     }
/// }
/// ```
pub trait CompletionSource: Send + Sync {
    /// Get all completion items from this source
    fn get_items(&self) -> Vec<CompletionItem>;

    /// Whether this source supports multi-select
    fn supports_multi_select(&self) -> bool {
        false
    }
}

/// Command source wrapper that implements CompletionSource
///
/// Provides completion items from a list of commands.
/// Single-select only (commands are executed one at a time).
pub struct CommandSource {
    items: Vec<CompletionItem>,
}

impl CommandSource {
    /// Create from a slash command registry
    pub fn from_registry(registry: &SlashCommandRegistry) -> Self {
        Self {
            items: command_source(registry),
        }
    }

    /// Create from a list of items
    pub fn new(items: Vec<CompletionItem>) -> Self {
        Self { items }
    }
}

impl CompletionSource for CommandSource {
    fn get_items(&self) -> Vec<CompletionItem> {
        self.items.clone()
    }

    fn supports_multi_select(&self) -> bool {
        false // Commands are single-select
    }
}

impl CompletionSource for FileSource {
    fn get_items(&self) -> Vec<CompletionItem> {
        // Use existing method
        FileSource::get_items(self)
    }

    fn supports_multi_select(&self) -> bool {
        true // Files support multi-select
    }
}

/// Convert slash commands from the registry into completion items
///
/// # Arguments
/// * `registry` - Reference to the slash command registry
///
/// # Returns
/// A vector of completion items, one for each registered command
pub fn command_source(registry: &SlashCommandRegistry) -> Vec<CompletionItem> {
    registry
        .list_all()
        .into_iter()
        .map(|cmd| CompletionItem {
            text: cmd.name,
            description: Some(cmd.description),
            item_type: CompletionType::Command,
        })
        .collect()
}

/// File source for completion
///
/// Enumerates files from a directory for completion. Does not recurse into subdirectories.
pub struct FileSource {
    /// Base directory to search
    pub directory: PathBuf,
    /// Optional file extensions to filter (e.g., ["md", "txt"])
    pub extensions: Option<Vec<String>>,
}

impl FileSource {
    /// Create a new file source
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
            extensions: None,
        }
    }

    /// Create a new file source with extension filtering
    pub fn with_extensions(directory: impl Into<PathBuf>, extensions: Vec<String>) -> Self {
        Self {
            directory: directory.into(),
            extensions: Some(extensions),
        }
    }

    /// Get completion items for files in the directory
    ///
    /// Returns an empty vector if:
    /// - Directory doesn't exist
    /// - Directory cannot be read
    /// - No files match the extension filter
    pub fn get_items(&self) -> Vec<CompletionItem> {
        let Ok(entries) = std::fs::read_dir(&self.directory) else {
            return Vec::new();
        };

        let mut items = Vec::new();

        for entry in entries.flatten() {
            let Ok(metadata) = entry.metadata() else {
                continue;
            };

            // Skip directories
            if metadata.is_dir() {
                continue;
            }

            let path = entry.path();

            // Apply extension filter if provided
            if let Some(ref exts) = self.extensions {
                let Some(extension) = path.extension() else {
                    continue;
                };
                let ext_str = extension.to_string_lossy().to_string();
                if !exts.contains(&ext_str) {
                    continue;
                }
            }

            // Get filename relative to directory
            if let Some(filename) = path.file_name() {
                let text = filename.to_string_lossy().to_string();
                items.push(CompletionItem::new(text, None, CompletionType::File));
            }
        }

        // Sort for consistent ordering
        items.sort_by(|a, b| a.text.cmp(&b.text));

        items
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_file_source_empty_directory() {
        // Create empty directory
        let temp_dir = TempDir::new().unwrap();
        let source = FileSource::new(temp_dir.path());

        let items = source.get_items();
        assert!(items.is_empty(), "Empty directory should return empty vec");
    }

    #[test]
    fn test_file_source_lists_files() {
        // Create directory with files
        let temp_dir = TempDir::new().unwrap();
        let dir_path = temp_dir.path();

        // Create some test files
        fs::write(dir_path.join("file1.txt"), "content1").unwrap();
        fs::write(dir_path.join("file2.md"), "content2").unwrap();
        fs::write(dir_path.join("file3.rs"), "content3").unwrap();

        let source = FileSource::new(dir_path);
        let items = source.get_items();

        assert_eq!(items.len(), 3, "Should find all 3 files");

        // Verify items are sorted
        assert_eq!(items[0].text, "file1.txt");
        assert_eq!(items[1].text, "file2.md");
        assert_eq!(items[2].text, "file3.rs");

        // Verify all are File type
        for item in &items {
            assert_eq!(item.item_type, CompletionType::File);
            assert!(item.description.is_none());
        }
    }

    #[test]
    fn test_file_source_with_extension_filter() {
        // Create directory with files
        let temp_dir = TempDir::new().unwrap();
        let dir_path = temp_dir.path();

        // Create test files with different extensions
        fs::write(dir_path.join("note1.md"), "content1").unwrap();
        fs::write(dir_path.join("note2.md"), "content2").unwrap();
        fs::write(dir_path.join("code.rs"), "content3").unwrap();
        fs::write(dir_path.join("data.txt"), "content4").unwrap();

        // Filter only .md files
        let source = FileSource::with_extensions(dir_path, vec!["md".to_string()]);
        let items = source.get_items();

        assert_eq!(items.len(), 2, "Should find only .md files");
        assert_eq!(items[0].text, "note1.md");
        assert_eq!(items[1].text, "note2.md");

        // Filter .rs and .txt files
        let source = FileSource::with_extensions(
            dir_path,
            vec!["rs".to_string(), "txt".to_string()],
        );
        let items = source.get_items();

        assert_eq!(items.len(), 2, "Should find .rs and .txt files");
        let texts: Vec<_> = items.iter().map(|i| i.text.as_str()).collect();
        assert!(texts.contains(&"code.rs"));
        assert!(texts.contains(&"data.txt"));
    }

    #[test]
    fn test_file_source_relative_paths() {
        // Create directory with files
        let temp_dir = TempDir::new().unwrap();
        let dir_path = temp_dir.path();

        fs::write(dir_path.join("test.md"), "content").unwrap();

        let source = FileSource::new(dir_path);
        let items = source.get_items();

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].text, "test.md", "Should return just the filename, not full path");

        // Verify it's not an absolute path
        assert!(!items[0].text.starts_with('/'));
        assert!(!items[0].text.contains("tmp"));
    }

    #[test]
    fn test_file_source_ignores_subdirectories() {
        // Create directory with files and subdirectories
        let temp_dir = TempDir::new().unwrap();
        let dir_path = temp_dir.path();

        fs::write(dir_path.join("root_file.txt"), "content").unwrap();

        let subdir = dir_path.join("subdir");
        fs::create_dir(&subdir).unwrap();
        fs::write(subdir.join("nested_file.txt"), "content").unwrap();

        let source = FileSource::new(dir_path);
        let items = source.get_items();

        // Should only find the root file, not the subdirectory or nested file
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].text, "root_file.txt");
    }

    #[test]
    fn test_file_source_nonexistent_directory() {
        let source = FileSource::new("/this/path/does/not/exist");
        let items = source.get_items();

        assert!(items.is_empty(), "Nonexistent directory should return empty vec");
    }

    #[test]
    fn test_file_source_no_matching_extensions() {
        let temp_dir = TempDir::new().unwrap();
        let dir_path = temp_dir.path();

        fs::write(dir_path.join("file.txt"), "content").unwrap();
        fs::write(dir_path.join("file.md"), "content").unwrap();

        // Filter for extensions that don't exist
        let source = FileSource::with_extensions(dir_path, vec!["rs".to_string()]);
        let items = source.get_items();

        assert!(items.is_empty(), "No files should match .rs extension");
    }

    #[test]
    fn test_file_source_files_without_extension() {
        let temp_dir = TempDir::new().unwrap();
        let dir_path = temp_dir.path();

        fs::write(dir_path.join("README"), "content").unwrap();
        fs::write(dir_path.join("Makefile"), "content").unwrap();
        fs::write(dir_path.join("file.txt"), "content").unwrap();

        // Without filter, should find all files
        let source = FileSource::new(dir_path);
        let items = source.get_items();

        assert_eq!(items.len(), 3);
        let texts: Vec<_> = items.iter().map(|i| i.text.as_str()).collect();
        assert!(texts.contains(&"README"));
        assert!(texts.contains(&"Makefile"));
        assert!(texts.contains(&"file.txt"));

        // With filter, should only find the one with extension
        let source = FileSource::with_extensions(dir_path, vec!["txt".to_string()]);
        let items = source.get_items();

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].text, "file.txt");
    }
}
