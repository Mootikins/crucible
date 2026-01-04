//! Integration test for NoteStore pipeline
//!
//! This test verifies the full pipeline from:
//! 1. MockNoteStore implementation
//! 2. Rune runtime with note store functions registered
//! 3. Script execution that queries the note store
//! 4. Result verification

use async_trait::async_trait;
use crucible_core::parser::BlockHash;
use crucible_core::storage::{
    Filter, NoteRecord, NoteStore, SearchResult, StorageError, StorageResult,
};
use crucible_rune::register_note_functions;
use rune::Module;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

// ============================================================================
// Mock NoteStore
// ============================================================================

/// Mock NoteStore that tracks calls and returns predetermined data
struct MockNoteStore {
    /// Notes to return from queries
    notes: Vec<NoteRecord>,
    /// Counter for get() calls (verifies the mock was actually called)
    get_call_count: AtomicUsize,
    /// Counter for list() calls
    list_call_count: AtomicUsize,
}

impl MockNoteStore {
    fn new(notes: Vec<NoteRecord>) -> Self {
        Self {
            notes,
            get_call_count: AtomicUsize::new(0),
            list_call_count: AtomicUsize::new(0),
        }
    }

    /// Create a store with sample test data
    fn with_test_data() -> Self {
        Self::new(vec![
            NoteRecord::new("test.md", BlockHash::zero())
                .with_title("Test Note")
                .with_tags(vec!["test".to_string(), "integration".to_string()])
                .with_links(vec!["other.md".to_string()]),
            NoteRecord::new("other.md", BlockHash::zero())
                .with_title("Other Note")
                .with_tags(vec!["reference".to_string()]),
            NoteRecord::new("notes/deep/nested.md", BlockHash::zero())
                .with_title("Nested Note")
                .with_tags(vec!["nested".to_string()]),
        ])
    }

    /// Get the number of times get() was called
    fn get_call_count(&self) -> usize {
        self.get_call_count.load(Ordering::SeqCst)
    }

    /// Get the number of times list() was called
    fn list_call_count(&self) -> usize {
        self.list_call_count.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl NoteStore for MockNoteStore {
    async fn upsert(&self, _note: NoteRecord) -> StorageResult<()> {
        Ok(())
    }

    async fn get(&self, path: &str) -> StorageResult<Option<NoteRecord>> {
        self.get_call_count.fetch_add(1, Ordering::SeqCst);
        Ok(self.notes.iter().find(|n| n.path == path).cloned())
    }

    async fn delete(&self, _path: &str) -> StorageResult<()> {
        Ok(())
    }

    async fn list(&self) -> StorageResult<Vec<NoteRecord>> {
        self.list_call_count.fetch_add(1, Ordering::SeqCst);
        Ok(self.notes.clone())
    }

    async fn get_by_hash(&self, _hash: &BlockHash) -> StorageResult<Option<NoteRecord>> {
        Ok(None)
    }

    async fn search(
        &self,
        _embedding: &[f32],
        _k: usize,
        _filter: Option<Filter>,
    ) -> StorageResult<Vec<SearchResult>> {
        Ok(vec![])
    }
}

// ============================================================================
// Helper: Create Rune module with note store functions
// ============================================================================

fn create_note_module(store: Arc<dyn NoteStore>) -> Result<Module, rune::compile::ContextError> {
    let mut module = Module::with_crate("graph")?;
    register_note_functions(&mut module, store)?;
    Ok(module)
}

/// Helper to compile and run async Rune script with note store module
async fn run_rune_with_note_store(
    store: Arc<dyn NoteStore>,
    script: &str,
) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
    use rune::termcolor::{ColorChoice, StandardStream};
    use rune::{Context, Diagnostics, Source, Sources, Vm};

    // Create module with note store functions
    let module = create_note_module(store)?;

    // Build context with the module
    let mut context = Context::with_default_modules()?;
    context.install(module)?;
    let runtime = std::sync::Arc::new(context.runtime()?);

    // Compile the script
    let mut sources = Sources::new();
    sources.insert(Source::new("test", script)?)?;

    let mut diagnostics = Diagnostics::new();
    let result = rune::prepare(&mut sources)
        .with_context(&context)
        .with_diagnostics(&mut diagnostics)
        .build();

    if !diagnostics.is_empty() {
        let mut writer = StandardStream::stderr(ColorChoice::Always);
        diagnostics.emit(&mut writer, &sources)?;
    }

    let unit = result?;
    let unit = std::sync::Arc::new(unit);

    // Execute asynchronously
    let vm = Vm::new(runtime, unit);
    let execution = vm.send_execute(["main"], ())?;
    let output = execution.async_complete().await.into_result()?;

    // Convert to JSON
    let json = crucible_rune::rune_to_json(&output)?;
    Ok(json)
}

// ============================================================================
// Integration Tests
// ============================================================================

/// Test the full pipeline: MockNoteStore -> Rune runtime -> script execution
#[tokio::test]
async fn test_notestore_pipeline_note_get() {
    let mock_store = Arc::new(MockNoteStore::with_test_data());
    let store: Arc<dyn NoteStore> = mock_store.clone();

    // Script that calls note_get("test.md")
    let script = r#"
        use graph::note_get;

        pub async fn main() {
            note_get("test.md").await
        }
    "#;

    let result = run_rune_with_note_store(store, script).await.unwrap();

    // Verify the mock was called
    assert_eq!(
        mock_store.get_call_count(),
        1,
        "get() should be called once"
    );

    // Verify the result came from NoteStore
    assert_eq!(result["path"], "test.md");
    assert_eq!(result["title"], "Test Note");

    // Check tags array
    let tags = result["tags"].as_array().expect("tags should be array");
    assert!(tags.contains(&serde_json::json!("test")));
    assert!(tags.contains(&serde_json::json!("integration")));
}

/// Test note_get returns null for non-existent notes
#[tokio::test]
async fn test_notestore_pipeline_note_get_missing() {
    let mock_store = Arc::new(MockNoteStore::with_test_data());
    let store: Arc<dyn NoteStore> = mock_store.clone();

    let script = r#"
        use graph::note_get;

        pub async fn main() {
            note_get("nonexistent.md").await
        }
    "#;

    let result = run_rune_with_note_store(store, script).await.unwrap();

    // Verify the mock was called
    assert_eq!(mock_store.get_call_count(), 1);

    // Should return null for missing note
    assert!(
        result.is_null(),
        "Expected null for missing note, got: {:?}",
        result
    );
}

/// Test note_list returns all notes
#[tokio::test]
async fn test_notestore_pipeline_note_list() {
    let mock_store = Arc::new(MockNoteStore::with_test_data());
    let store: Arc<dyn NoteStore> = mock_store.clone();

    let script = r#"
        use graph::note_list;

        pub async fn main() {
            note_list(0).await  // 0 = no limit
        }
    "#;

    let result = run_rune_with_note_store(store, script).await.unwrap();

    // Verify the mock was called
    assert_eq!(
        mock_store.list_call_count(),
        1,
        "list() should be called once"
    );

    // Verify result is an array with 3 notes
    let arr = result.as_array().expect("Should be array");
    assert_eq!(arr.len(), 3, "Should return all 3 notes");

    // Verify note paths are present
    let paths: Vec<&str> = arr.iter().filter_map(|n| n["path"].as_str()).collect();
    assert!(paths.contains(&"test.md"));
    assert!(paths.contains(&"other.md"));
    assert!(paths.contains(&"notes/deep/nested.md"));
}

/// Test note_list respects limit parameter
#[tokio::test]
async fn test_notestore_pipeline_note_list_with_limit() {
    let mock_store = Arc::new(MockNoteStore::with_test_data());
    let store: Arc<dyn NoteStore> = mock_store.clone();

    let script = r#"
        use graph::note_list;

        pub async fn main() {
            note_list(2).await  // limit to 2
        }
    "#;

    let result = run_rune_with_note_store(store, script).await.unwrap();

    // Verify the mock was called
    assert_eq!(mock_store.list_call_count(), 1);

    // Verify result is limited to 2 notes
    let arr = result.as_array().expect("Should be array");
    assert_eq!(arr.len(), 2, "Should return only 2 notes due to limit");
}

/// Test that note data flows correctly through the pipeline
#[tokio::test]
async fn test_notestore_pipeline_data_integrity() {
    let mock_store = Arc::new(MockNoteStore::with_test_data());
    let store: Arc<dyn NoteStore> = mock_store.clone();

    // Script that gets a note and extracts specific fields
    let script = r#"
        use graph::note_get;

        pub async fn main() {
            let note = note_get("test.md").await;
            #{
                path: note.path,
                title: note.title,
                tag_count: note.tags.len() as i64,
                link_count: note.links_to.len() as i64,
            }
        }
    "#;

    let result = run_rune_with_note_store(store, script).await.unwrap();

    assert_eq!(result["path"], "test.md");
    assert_eq!(result["title"], "Test Note");
    assert_eq!(result["tag_count"], 2);
    assert_eq!(result["link_count"], 1);
}

/// Test combining note_get and note_list in same script
#[tokio::test]
async fn test_notestore_pipeline_combined_operations() {
    let mock_store = Arc::new(MockNoteStore::with_test_data());
    let store: Arc<dyn NoteStore> = mock_store.clone();

    let script = r#"
        use graph::{note_get, note_list};

        pub async fn main() {
            let all_notes = note_list(0).await;
            let specific = note_get("other.md").await;
            #{
                total_count: all_notes.len() as i64,
                specific_title: specific.title,
            }
        }
    "#;

    let result = run_rune_with_note_store(store, script).await.unwrap();

    // Verify both methods were called
    assert_eq!(mock_store.get_call_count(), 1);
    assert_eq!(mock_store.list_call_count(), 1);

    assert_eq!(result["total_count"], 3);
    assert_eq!(result["specific_title"], "Other Note");
}

/// Test error handling when NoteStore fails
#[tokio::test]
async fn test_notestore_pipeline_error_propagation() {
    /// A NoteStore that always fails
    struct FailingNoteStore;

    #[async_trait]
    impl NoteStore for FailingNoteStore {
        async fn upsert(&self, _note: NoteRecord) -> StorageResult<()> {
            Err(StorageError::backend("Store unavailable"))
        }

        async fn get(&self, _path: &str) -> StorageResult<Option<NoteRecord>> {
            Err(StorageError::backend("Connection lost"))
        }

        async fn delete(&self, _path: &str) -> StorageResult<()> {
            Err(StorageError::backend("Store unavailable"))
        }

        async fn list(&self) -> StorageResult<Vec<NoteRecord>> {
            Err(StorageError::backend("Store unavailable"))
        }

        async fn get_by_hash(&self, _hash: &BlockHash) -> StorageResult<Option<NoteRecord>> {
            Err(StorageError::backend("Store unavailable"))
        }

        async fn search(
            &self,
            _embedding: &[f32],
            _k: usize,
            _filter: Option<Filter>,
        ) -> StorageResult<Vec<SearchResult>> {
            Err(StorageError::backend("Store unavailable"))
        }
    }

    let store: Arc<dyn NoteStore> = Arc::new(FailingNoteStore);

    let script = r#"
        use graph::note_get;

        pub async fn main() {
            note_get("any.md").await
        }
    "#;

    let result = run_rune_with_note_store(store, script).await;
    assert!(result.is_err(), "Should propagate storage error");

    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("Connection lost"),
        "Error should contain original message, got: {}",
        err
    );
}

/// Test using RuneExecutor with note store module (alternative API)
///
/// This test demonstrates how to use RuneExecutor::with_modules() to add
/// custom modules like the note store functions.
#[tokio::test]
async fn test_notestore_with_rune_executor() {
    use rune::{Context, Diagnostics, Source, Sources, Vm};

    let mock_store = Arc::new(MockNoteStore::with_test_data());
    let store: Arc<dyn NoteStore> = mock_store.clone();

    // Create the note module
    let module = create_note_module(store).expect("Should create module");

    // Build context manually (RuneExecutor::with_modules adds other modules that conflict)
    let mut context = Context::with_default_modules().expect("Default modules");
    context.install(module).expect("Install note module");
    let runtime = std::sync::Arc::new(context.runtime().expect("Runtime"));

    // Compile the script
    let source = r#"
        use graph::note_get;

        pub async fn main() {
            note_get("test.md").await
        }
    "#;

    let mut sources = Sources::new();
    sources
        .insert(Source::new("test", source).unwrap())
        .unwrap();
    let mut diagnostics = Diagnostics::new();

    let unit = rune::prepare(&mut sources)
        .with_context(&context)
        .with_diagnostics(&mut diagnostics)
        .build()
        .expect("Should compile");

    let unit = std::sync::Arc::new(unit);

    // Execute using send_execute for proper async handling
    let vm = Vm::new(runtime, unit);
    let execution = vm.send_execute(["main"], ()).expect("Send execute");
    let output = execution
        .async_complete()
        .await
        .into_result()
        .expect("Complete");

    // Convert to JSON
    let result = crucible_rune::rune_to_json(&output).expect("To JSON");

    // Verify
    assert_eq!(mock_store.get_call_count(), 1);
    assert_eq!(result["path"], "test.md");
    assert_eq!(result["title"], "Test Note");
}
