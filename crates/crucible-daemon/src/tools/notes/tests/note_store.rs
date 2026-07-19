//! `NoteStore` integration tests for `NoteTools`.
//!
//! These tests verify the `NoteStore` code path works correctly.

use super::super::{ListNotesParams, NoteTools, ReadMetadataParams};
use async_trait::async_trait;
use chrono::Utc;
use crucible_core::events::{InternalSessionEvent, NoteChangeType, SessionEvent};
use crucible_core::parser::BlockHash;
use crucible_core::storage::{Filter, NoteRecord, NoteStore, StorageResult};
use rmcp::handler::server::wrapper::Parameters;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tempfile::TempDir;

/// Mock `NoteStore` for testing the `NoteStore` integration path
struct MockNoteStore {
    notes: Mutex<HashMap<String, NoteRecord>>,
}

impl MockNoteStore {
    fn new() -> Self {
        Self {
            notes: Mutex::new(HashMap::new()),
        }
    }

    fn add_note(&self, record: NoteRecord) {
        let mut notes = self.notes.lock().unwrap();
        notes.insert(record.path.clone(), record);
    }
}

#[async_trait]
impl NoteStore for MockNoteStore {
    async fn upsert(&self, note: NoteRecord) -> StorageResult<Vec<SessionEvent>> {
        self.add_note(note.clone());
        let existed = self.notes.lock().unwrap().contains_key(&note.path);
        let event = if existed {
            SessionEvent::internal(InternalSessionEvent::NoteModified {
                path: note.path.into(),
                change_type: NoteChangeType::Content,
            })
        } else {
            SessionEvent::internal(InternalSessionEvent::NoteCreated {
                path: note.path.into(),
                title: Some(note.title),
            })
        };
        Ok(vec![event])
    }

    async fn get(
        &self,
        path: &str,
        _authority: &crucible_core::storage::Scope,
    ) -> StorageResult<Option<NoteRecord>> {
        let notes = self.notes.lock().unwrap();
        Ok(notes.get(path).cloned())
    }

    async fn delete(&self, path: &str) -> StorageResult<SessionEvent> {
        let mut notes = self.notes.lock().unwrap();
        let existed = notes.remove(path).is_some();
        Ok(SessionEvent::internal(InternalSessionEvent::NoteDeleted {
            path: path.into(),
            existed,
        }))
    }

    async fn list(
        &self,
        _authority: &crucible_core::storage::Scope,
    ) -> StorageResult<Vec<NoteRecord>> {
        let notes = self.notes.lock().unwrap();
        Ok(notes.values().cloned().collect())
    }

    async fn get_by_hash(
        &self,
        _hash: &BlockHash,
        _authority: &crucible_core::storage::Scope,
    ) -> StorageResult<Option<NoteRecord>> {
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

#[tokio::test]
async fn test_read_metadata_uses_note_store() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();

    // Create a mock NoteStore with a pre-populated note
    let mock_store = Arc::new(MockNoteStore::new());

    let mut properties = HashMap::new();
    properties.insert("status".to_string(), serde_json::json!("published"));

    mock_store.add_note(NoteRecord {
        path: "test.md".to_string(),
        content_hash: BlockHash::zero(),
        embedding: Some(vec![0.1; 384]),
        embedding_model: None,
        embedding_dimensions: None,
        title: "Test Note from Index".to_string(),
        tags: vec!["rust".to_string(), "test".to_string()],
        links_to: vec!["other.md".to_string()],
        links: Vec::new(),
        properties,
        updated_at: Utc::now(),
    });

    // Create NoteTools with the mock store
    let note_tools = NoteTools::with_note_store(kiln_path, mock_store);

    // Read metadata - should use the NoteStore, not filesystem
    let result = note_tools
        .read_metadata(Parameters(ReadMetadataParams {
            path: "test.md".to_string(),
        }))
        .await;

    assert!(result.is_ok(), "read_metadata should succeed");

    let call_result = result.unwrap();
    if let Some(content) = call_result.content.first() {
        if let Some(raw_text) = content.as_text() {
            let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();

            // Verify data came from NoteStore (has "source": "index")
            assert_eq!(parsed["source"], "index", "Should indicate index source");
            assert_eq!(parsed["frontmatter"]["title"], "Test Note from Index");
            assert_eq!(parsed["frontmatter"]["status"], "published");

            // Verify stats from NoteStore
            assert_eq!(parsed["stats"]["links_count"], 1);
            assert_eq!(parsed["stats"]["tags_count"], 2);
            assert_eq!(parsed["stats"]["has_embedding"], true);
        }
    }
}

#[tokio::test]
async fn test_read_metadata_fallback_to_filesystem() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();

    // Create a mock NoteStore that doesn't have the note
    let mock_store = Arc::new(MockNoteStore::new());

    // Create a file on the filesystem
    let note_content = "---\ntitle: Filesystem Note\ntags: [fs]\n---\n\n# Content";
    std::fs::write(temp_dir.path().join("fs-note.md"), note_content).unwrap();

    // Create NoteTools with the mock store
    let note_tools = NoteTools::with_note_store(kiln_path, mock_store);

    // Read metadata - should fall back to filesystem since note not in store
    let result = note_tools
        .read_metadata(Parameters(ReadMetadataParams {
            path: "fs-note.md".to_string(),
        }))
        .await;

    assert!(result.is_ok(), "read_metadata should succeed via fallback");

    let call_result = result.unwrap();
    if let Some(content) = call_result.content.first() {
        if let Some(raw_text) = content.as_text() {
            let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();

            // Should NOT have "source": "index" since it came from filesystem
            assert!(
                parsed.get("source").is_none(),
                "Should not have index source"
            );
            assert_eq!(parsed["frontmatter"]["title"], "Filesystem Note");
        }
    }
}

#[tokio::test]
async fn test_list_notes_uses_note_store() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();

    // Create a mock NoteStore with multiple notes
    let mock_store = Arc::new(MockNoteStore::new());

    mock_store.add_note(NoteRecord {
        path: "note1.md".to_string(),
        content_hash: BlockHash::zero(),
        embedding: None,
        embedding_model: None,
        embedding_dimensions: None,
        title: "Note 1".to_string(),
        tags: vec!["tag1".to_string()],
        links_to: vec![],
        links: Vec::new(),
        properties: HashMap::new(),
        updated_at: Utc::now(),
    });

    mock_store.add_note(NoteRecord {
        path: "folder/note2.md".to_string(),
        content_hash: BlockHash::zero(),
        embedding: None,
        embedding_model: None,
        embedding_dimensions: None,
        title: "Note 2".to_string(),
        tags: vec!["tag2".to_string()],
        links_to: vec![],
        links: Vec::new(),
        properties: HashMap::new(),
        updated_at: Utc::now(),
    });

    // Create NoteTools with the mock store
    let note_tools = NoteTools::with_note_store(kiln_path, mock_store);

    // List notes - should use the NoteStore
    let result = note_tools
        .list_notes(Parameters(ListNotesParams {
            folder: None,
            include_frontmatter: true,
            recursive: true,
        }))
        .await;

    assert!(result.is_ok(), "list_notes should succeed");

    let call_result = result.unwrap();
    if let Some(content) = call_result.content.first() {
        if let Some(raw_text) = content.as_text() {
            let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();

            // Verify data came from NoteStore
            assert_eq!(parsed["source"], "index", "Should indicate index source");
            assert_eq!(parsed["count"], 2);

            let notes = parsed["notes"].as_array().unwrap();
            assert_eq!(notes.len(), 2);
        }
    }
}

#[tokio::test]
async fn test_list_notes_filters_by_folder() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();

    // Create a mock NoteStore with notes in different folders
    let mock_store = Arc::new(MockNoteStore::new());

    mock_store.add_note(NoteRecord {
        path: "root.md".to_string(),
        content_hash: BlockHash::zero(),
        embedding: None,
        embedding_model: None,
        embedding_dimensions: None,
        title: "Root Note".to_string(),
        tags: vec![],
        links_to: vec![],
        links: Vec::new(),
        properties: HashMap::new(),
        updated_at: Utc::now(),
    });

    mock_store.add_note(NoteRecord {
        path: "projects/rust.md".to_string(),
        content_hash: BlockHash::zero(),
        embedding: None,
        embedding_model: None,
        embedding_dimensions: None,
        title: "Rust Project".to_string(),
        tags: vec![],
        links_to: vec![],
        links: Vec::new(),
        properties: HashMap::new(),
        updated_at: Utc::now(),
    });

    mock_store.add_note(NoteRecord {
        path: "projects/python.md".to_string(),
        content_hash: BlockHash::zero(),
        embedding: None,
        embedding_model: None,
        embedding_dimensions: None,
        title: "Python Project".to_string(),
        tags: vec![],
        links_to: vec![],
        links: Vec::new(),
        properties: HashMap::new(),
        updated_at: Utc::now(),
    });

    // Create the folder on filesystem (required for validation)
    std::fs::create_dir_all(temp_dir.path().join("projects")).unwrap();

    // Create NoteTools with the mock store
    let note_tools = NoteTools::with_note_store(kiln_path, mock_store);

    // List notes in projects folder only
    let result = note_tools
        .list_notes(Parameters(ListNotesParams {
            folder: Some("projects".to_string()),
            include_frontmatter: false,
            recursive: true,
        }))
        .await;

    assert!(result.is_ok(), "list_notes should succeed");

    let call_result = result.unwrap();
    if let Some(content) = call_result.content.first() {
        if let Some(raw_text) = content.as_text() {
            let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();

            // Should only return notes in the projects folder
            assert_eq!(parsed["count"], 2);
            assert_eq!(parsed["folder"], "projects");
        }
    }
}

#[tokio::test]
async fn test_list_notes_non_recursive_with_store() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();

    // Create a mock NoteStore with notes at different levels
    let mock_store = Arc::new(MockNoteStore::new());

    mock_store.add_note(NoteRecord {
        path: "root.md".to_string(),
        content_hash: BlockHash::zero(),
        embedding: None,
        embedding_model: None,
        embedding_dimensions: None,
        title: "Root".to_string(),
        tags: vec![],
        links_to: vec![],
        links: Vec::new(),
        properties: HashMap::new(),
        updated_at: Utc::now(),
    });

    mock_store.add_note(NoteRecord {
        path: "nested/deep.md".to_string(),
        content_hash: BlockHash::zero(),
        embedding: None,
        embedding_model: None,
        embedding_dimensions: None,
        title: "Deep".to_string(),
        tags: vec![],
        links_to: vec![],
        links: Vec::new(),
        properties: HashMap::new(),
        updated_at: Utc::now(),
    });

    let note_tools = NoteTools::with_note_store(kiln_path, mock_store);

    // List notes non-recursively at root
    let result = note_tools
        .list_notes(Parameters(ListNotesParams {
            folder: None,
            include_frontmatter: false,
            recursive: false,
        }))
        .await;

    assert!(result.is_ok());

    let call_result = result.unwrap();
    if let Some(content) = call_result.content.first() {
        if let Some(raw_text) = content.as_text() {
            let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();

            // Non-recursive should only return root.md
            assert_eq!(parsed["count"], 1);
            assert_eq!(parsed["recursive"], false);
        }
    }
}
