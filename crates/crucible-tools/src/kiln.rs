//! Kiln operations tools
//!
//! This module provides kiln-specific tools like roots and statistics.

#![allow(missing_docs)]

use std::collections::HashSet;
use std::sync::Arc;

use crate::helpers::json_success;
use crucible_core::storage::NoteStore;
use rmcp::{model::CallToolResult, tool, tool_router};

#[derive(Clone)]
#[allow(missing_docs)]
pub struct KilnTools {
    kiln_path: String,
    note_store: Option<Arc<dyn NoteStore>>,
}

impl KilnTools {
    #[allow(missing_docs)]
    #[must_use]
    pub fn new(kiln_path: String) -> Self {
        Self {
            kiln_path,
            note_store: None,
        }
    }

    /// Create `KilnTools` with a `NoteStore` for accurate indexed statistics
    ///
    /// When a `NoteStore` is provided, `get_kiln_info` returns statistics from
    /// the indexed database instead of walking the filesystem.
    #[must_use]
    pub fn with_note_store(kiln_path: String, note_store: Arc<dyn NoteStore>) -> Self {
        Self {
            kiln_path,
            note_store: Some(note_store),
        }
    }
}

#[tool_router]
impl KilnTools {
    #[tool(description = "Get comprehensive kiln information")]
    pub async fn get_kiln_info(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        // Extract kiln name from path
        let name = std::path::Path::new(&self.kiln_path)
            .file_name()
            .map_or_else(
                || "unknown".to_string(),
                |n| n.to_string_lossy().into_owned(),
            );

        // Use indexed data from NoteStore when available
        if let Some(store) = &self.note_store {
            let notes = store.list().await.map_err(|e| rmcp::ErrorData {
                code: rmcp::model::ErrorCode(-32603), // INTERNAL_ERROR
                message: format!("Failed to list notes: {e}").into(),
                data: None,
            })?;

            let indexed_notes = notes.len();
            let embedded_notes = notes.iter().filter(|n| n.has_embedding()).count();
            let mut tags = HashSet::new();
            let mut total_links = 0;
            for note in &notes {
                for tag in &note.tags {
                    tags.insert(tag.clone());
                }
                total_links += note.links_to.len();
            }

            return json_success(serde_json::json!({
                "name": name,
                "indexed_notes": indexed_notes,
                "embedded_notes": embedded_notes,
                "unique_tags": tags.len(),
                "total_links": total_links,
            }));
        }

        // Fallback: walk filesystem when no NoteStore available
        let mut total_files = 0;
        let mut total_size = 0;
        let mut md_files = 0;

        for entry in walkdir::WalkDir::new(&self.kiln_path)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| {
                if e.file_type().is_dir() {
                    !e.file_name().to_string_lossy().starts_with('.') || e.depth() == 0
                } else {
                    true
                }
            })
            .filter_map(std::result::Result::ok)
            .filter(|e| e.file_type().is_file())
        {
            total_files += 1;
            if let Ok(metadata) = entry.metadata() {
                total_size += metadata.len();
            }
            if entry.path().extension().is_some_and(|ext| ext == "md") {
                md_files += 1;
            }
        }

        json_success(serde_json::json!({
            "name": name,
            "total_files": total_files,
            "markdown_files": md_files,
            "total_size_bytes": total_size
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_kiln_tools_creation() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        let kiln_tools = KilnTools::new(kiln_path);
        assert_eq!(kiln_tools.kiln_path, temp_dir.path().to_string_lossy());
    }

    #[tokio::test]
    async fn test_get_kiln_info_empty() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        let kiln_tools = KilnTools::new(kiln_path.clone());

        let result = kiln_tools.get_kiln_info().await;
        assert!(result.is_ok());

        let call_result = result.unwrap();
        if let Some(content) = call_result.content.first() {
            let raw_text = content.as_text().unwrap();
            let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();

            // Check flat structure
            assert!(parsed["name"].as_str().is_some());
            assert_eq!(parsed["total_files"], 0);
            assert_eq!(parsed["markdown_files"], 0);
            assert_eq!(parsed["total_size_bytes"], 0);

            // Verify no nested root or path fields
            assert!(parsed.get("root").is_none() || parsed["root"].is_null());
            assert!(parsed.get("path").is_none() || parsed["path"].is_null());
        }
    }

    #[tokio::test]
    async fn test_get_kiln_info_with_files() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        let kiln_tools = KilnTools::new(kiln_path.clone());

        // Create some test files
        std::fs::write(temp_dir.path().join("test1.md"), "# Test Note 1").unwrap();
        std::fs::write(
            temp_dir.path().join("test2.md"),
            "# Test Note 2\nWith more content.",
        )
        .unwrap();
        std::fs::write(temp_dir.path().join("ignore.txt"), "Ignore me").unwrap();

        let result = kiln_tools.get_kiln_info().await;
        assert!(result.is_ok());

        let call_result = result.unwrap();
        if let Some(content) = call_result.content.first() {
            let raw_text = content.as_text().unwrap();
            let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();

            // Check flat structure
            assert!(parsed["name"].as_str().is_some());
            assert_eq!(parsed["total_files"], 3);
            assert_eq!(parsed["markdown_files"], 2);
            assert!(parsed["total_size_bytes"].as_u64().unwrap() > 0);

            // Verify no nested root or path fields
            assert!(parsed.get("root").is_none() || parsed["root"].is_null());
            assert!(parsed.get("path").is_none() || parsed["path"].is_null());
        }
    }

    #[test]
    fn test_tool_router_creation() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        let _kiln_tools = KilnTools::new(kiln_path);

        // This should compile and not panic - the tool_router macro generates the router
        let _router = KilnTools::tool_router();
    }

    #[tokio::test]
    async fn test_get_kiln_info_uses_note_store() {
        use async_trait::async_trait;
        use crucible_core::events::{InternalSessionEvent, SessionEvent};
        use crucible_core::parser::BlockHash;
        use crucible_core::storage::{Filter, NoteRecord, NoteStore, StorageResult};
        use std::sync::{Arc, Mutex};

        struct MockNoteStore {
            notes: Mutex<Vec<NoteRecord>>,
        }

        impl MockNoteStore {
            fn new(notes: Vec<NoteRecord>) -> Self {
                Self {
                    notes: Mutex::new(notes),
                }
            }
        }

        #[async_trait]
        impl NoteStore for MockNoteStore {
            async fn upsert(&self, _note: NoteRecord) -> StorageResult<Vec<SessionEvent>> {
                Ok(vec![])
            }
            async fn get(&self, _path: &str) -> StorageResult<Option<NoteRecord>> {
                Ok(None)
            }
            async fn delete(&self, path: &str) -> StorageResult<SessionEvent> {
                Ok(SessionEvent::internal(InternalSessionEvent::NoteDeleted {
                    path: path.into(),
                    existed: false,
                }))
            }
            async fn list(&self) -> StorageResult<Vec<NoteRecord>> {
                Ok(self.notes.lock().unwrap().clone())
            }
            async fn get_by_hash(&self, _hash: &BlockHash) -> StorageResult<Option<NoteRecord>> {
                Ok(None)
            }
            async fn search(
                &self,
                _embedding: &[f32],
                _k: usize,
                _filter: Option<Filter>,
            ) -> StorageResult<Vec<crucible_core::storage::note_store::SearchResult>> {
                Ok(vec![])
            }
        }

        let notes = vec![
            NoteRecord::new("notes/alpha.md", BlockHash::zero())
                .with_title("Alpha")
                .with_tags(vec!["rust".into(), "dev".into()])
                .with_links(vec!["notes/beta.md".into()]),
            NoteRecord::new("notes/beta.md", BlockHash::zero())
                .with_title("Beta")
                .with_tags(vec!["rust".into()])
                .with_embedding(vec![0.1; 768]),
            NoteRecord::new("archive/old.md", BlockHash::zero())
                .with_title("Old Note")
                .with_tags(vec!["archive".into()]),
        ];

        let store = Arc::new(MockNoteStore::new(notes));
        let kiln_tools = KilnTools::with_note_store("/tmp/test-kiln".into(), store);

        let result = kiln_tools.get_kiln_info().await.unwrap();
        let content = result.content.first().unwrap();
        let raw_text = content.as_text().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();

        assert_eq!(parsed["name"], "test-kiln");
        assert_eq!(parsed["indexed_notes"], 3);
        assert_eq!(parsed["embedded_notes"], 1);
        assert_eq!(parsed["unique_tags"], 3); // rust, dev, archive
        assert_eq!(parsed["total_links"], 1);
    }

    #[tokio::test]
    async fn test_get_kiln_info_recursive() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        // Create nested structure
        std::fs::create_dir_all(temp_dir.path().join("sub/deep")).unwrap();
        std::fs::create_dir_all(temp_dir.path().join(".hidden")).unwrap();

        std::fs::write(temp_dir.path().join("root.md"), "# Root").unwrap();
        std::fs::write(temp_dir.path().join("sub/nested.md"), "# Nested").unwrap();
        std::fs::write(temp_dir.path().join("sub/deep/inner.md"), "# Inner").unwrap();
        std::fs::write(temp_dir.path().join("other.txt"), "text file").unwrap();
        std::fs::write(temp_dir.path().join(".hidden/secret.md"), "# Secret").unwrap(); // must be excluded

        let kiln_tools = KilnTools::new(kiln_path);
        let result = kiln_tools.get_kiln_info().await;
        assert!(result.is_ok());

        let call_result = result.unwrap();
        if let Some(content) = call_result.content.first() {
            let raw_text = content.as_text().unwrap();
            let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();

            assert_eq!(parsed["total_files"], 4, "should count root.md + sub/nested.md + sub/deep/inner.md + other.txt, excluding .hidden/");
            assert_eq!(parsed["markdown_files"], 3, "should count root.md + sub/nested.md + sub/deep/inner.md, excluding .hidden/secret.md");
        }
    }
}
