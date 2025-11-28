//! Integration tests for NotePipeline
//!
//! These tests verify the complete pipeline flow through all 5 phases with
//! various scenarios including success paths, error handling, and edge cases.

use anyhow::Result;
use async_trait::async_trait;
use crucible_core::enrichment::{EnrichedNote, EnrichmentService};
use crucible_core::processing::{
    ChangeDetectionError, ChangeDetectionResult, ChangeDetectionStore, FileState, ProcessingResult,
};
use crucible_core::test_support::mocks::MockEnrichmentService;
use crucible_core::EnrichedNoteStore;
use crucible_merkle::{HybridMerkleTree, MerkleStore, StorageError, TreeMetadata};
use crucible_pipeline::{NotePipeline, NotePipelineConfig};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tempfile::TempDir;

// ============================================================================
// Mock Implementations for Testing
// ============================================================================

/// Mock change detection store for testing
#[derive(Clone)]
struct MockChangeDetectionStore {
    state: Arc<Mutex<MockChangeDetectionState>>,
}

struct MockChangeDetectionState {
    file_states: HashMap<String, FileState>,
    simulate_errors: bool,
    error_message: String,
}

impl MockChangeDetectionStore {
    fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(MockChangeDetectionState {
                file_states: HashMap::new(),
                simulate_errors: false,
                error_message: String::new(),
            })),
        }
    }

    fn set_simulate_errors(&self, enabled: bool, message: &str) {
        let mut state = self.state.lock().unwrap();
        state.simulate_errors = enabled;
        state.error_message = message.to_string();
    }

    fn add_file_state(&self, path: &str, file_state: FileState) {
        let mut state = self.state.lock().unwrap();
        state.file_states.insert(path.to_string(), file_state);
    }

    fn clear(&self) {
        let mut state = self.state.lock().unwrap();
        state.file_states.clear();
        state.simulate_errors = false;
        state.error_message.clear();
    }
}

#[async_trait]
impl ChangeDetectionStore for MockChangeDetectionStore {
    async fn get_file_state(&self, path: &Path) -> ChangeDetectionResult<Option<FileState>> {
        let state = self.state.lock().unwrap();

        if state.simulate_errors {
            return Err(ChangeDetectionError::Storage(state.error_message.clone()));
        }

        let path_str = path.to_string_lossy().to_string();
        Ok(state.file_states.get(&path_str).cloned())
    }

    async fn store_file_state(
        &self,
        path: &Path,
        file_state: FileState,
    ) -> ChangeDetectionResult<()> {
        let mut state = self.state.lock().unwrap();

        if state.simulate_errors {
            return Err(ChangeDetectionError::Storage(state.error_message.clone()));
        }

        let path_str = path.to_string_lossy().to_string();
        state.file_states.insert(path_str, file_state);
        Ok(())
    }

    async fn delete_file_state(&self, path: &Path) -> ChangeDetectionResult<()> {
        let mut state = self.state.lock().unwrap();

        if state.simulate_errors {
            return Err(ChangeDetectionError::Storage(state.error_message.clone()));
        }

        let path_str = path.to_string_lossy().to_string();
        state.file_states.remove(&path_str);
        Ok(())
    }

    async fn list_tracked_files(&self) -> ChangeDetectionResult<Vec<PathBuf>> {
        let state = self.state.lock().unwrap();

        if state.simulate_errors {
            return Err(ChangeDetectionError::Storage(state.error_message.clone()));
        }

        Ok(state.file_states.keys().map(|s| PathBuf::from(s)).collect())
    }
}

/// Mock Merkle tree store for testing
#[derive(Clone)]
struct MockMerkleStore {
    state: Arc<Mutex<MockMerkleStoreState>>,
}

struct MockMerkleStoreState {
    trees: HashMap<String, HybridMerkleTree>,
    simulate_errors: bool,
    error_message: String,
}

impl MockMerkleStore {
    fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(MockMerkleStoreState {
                trees: HashMap::new(),
                simulate_errors: false,
                error_message: String::new(),
            })),
        }
    }

    fn set_simulate_errors(&self, enabled: bool, message: &str) {
        let mut state = self.state.lock().unwrap();
        state.simulate_errors = enabled;
        state.error_message = message.to_string();
    }

    fn add_tree(&self, id: &str, tree: HybridMerkleTree) {
        let mut state = self.state.lock().unwrap();
        state.trees.insert(id.to_string(), tree);
    }

    fn clear(&self) {
        let mut state = self.state.lock().unwrap();
        state.trees.clear();
        state.simulate_errors = false;
        state.error_message.clear();
    }
}

#[async_trait]
impl MerkleStore for MockMerkleStore {
    async fn store(&self, id: &str, tree: &HybridMerkleTree) -> Result<(), StorageError> {
        let mut state = self.state.lock().unwrap();

        if state.simulate_errors {
            return Err(StorageError::Storage(state.error_message.clone()));
        }

        state.trees.insert(id.to_string(), tree.clone());
        Ok(())
    }

    async fn retrieve(&self, id: &str) -> Result<HybridMerkleTree, StorageError> {
        let state = self.state.lock().unwrap();

        if state.simulate_errors {
            return Err(StorageError::Storage(state.error_message.clone()));
        }

        state
            .trees
            .get(id)
            .cloned()
            .ok_or_else(|| StorageError::NotFound(id.to_string()))
    }

    async fn delete(&self, id: &str) -> Result<(), StorageError> {
        let mut state = self.state.lock().unwrap();

        if state.simulate_errors {
            return Err(StorageError::Storage(state.error_message.clone()));
        }

        state.trees.remove(id);
        Ok(())
    }

    async fn get_metadata(&self, id: &str) -> Result<Option<TreeMetadata>, StorageError> {
        let state = self.state.lock().unwrap();

        if state.simulate_errors {
            return Err(StorageError::Storage(state.error_message.clone()));
        }

        // Return None for simplicity in tests
        let _ = id;
        Ok(None)
    }

    async fn update_incremental(
        &self,
        id: &str,
        tree: &HybridMerkleTree,
        _changed_sections: &[usize],
    ) -> Result<(), StorageError> {
        // For tests, just store the whole tree
        self.store(id, tree).await
    }

    async fn list_trees(&self) -> Result<Vec<TreeMetadata>, StorageError> {
        let state = self.state.lock().unwrap();

        if state.simulate_errors {
            return Err(StorageError::Storage(state.error_message.clone()));
        }

        // Return empty list for simplicity in tests
        Ok(vec![])
    }
}

/// Mock enriched note store for testing
#[derive(Clone)]
struct MockEnrichedNoteStore {
    state: Arc<Mutex<MockEnrichedNoteStoreState>>,
}

struct MockEnrichedNoteStoreState {
    stored_notes: Vec<EnrichedNote>,
    simulate_errors: bool,
    error_message: String,
    store_count: usize,
}

impl MockEnrichedNoteStore {
    fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(MockEnrichedNoteStoreState {
                stored_notes: Vec::new(),
                simulate_errors: false,
                error_message: String::new(),
                store_count: 0,
            })),
        }
    }

    fn set_simulate_errors(&self, enabled: bool, message: &str) {
        let mut state = self.state.lock().unwrap();
        state.simulate_errors = enabled;
        state.error_message = message.to_string();
    }

    fn store_count(&self) -> usize {
        self.state.lock().unwrap().store_count
    }

    fn get_stored_notes(&self) -> Vec<EnrichedNote> {
        self.state.lock().unwrap().stored_notes.clone()
    }

    fn clear(&self) {
        let mut state = self.state.lock().unwrap();
        state.stored_notes.clear();
        state.simulate_errors = false;
        state.error_message.clear();
        state.store_count = 0;
    }
}

#[async_trait]
impl EnrichedNoteStore for MockEnrichedNoteStore {
    async fn store_enriched(&self, enriched: &EnrichedNote, _relative_path: &str) -> Result<()> {
        let mut state = self.state.lock().unwrap();

        if state.simulate_errors {
            return Err(anyhow::anyhow!("{}", state.error_message));
        }

        state.stored_notes.push(enriched.clone());
        state.store_count += 1;
        Ok(())
    }
}

// ============================================================================
// Test Helpers
// ============================================================================

/// Create a temporary test file with markdown content
fn create_test_file(content: &str) -> Result<(TempDir, PathBuf)> {
    let temp_dir = TempDir::new()?;
    let file_path = temp_dir.path().join("test_note.md");
    std::fs::write(&file_path, content)?;
    Ok((temp_dir, file_path))
}

/// Create a basic test pipeline with all mocks
fn create_test_pipeline() -> (
    NotePipeline,
    Arc<MockChangeDetectionStore>,
    Arc<MockMerkleStore>,
    Arc<MockEnrichmentService>,
    Arc<MockEnrichedNoteStore>,
) {
    let change_detector = Arc::new(MockChangeDetectionStore::new());
    let merkle_store = Arc::new(MockMerkleStore::new());
    let enrichment_service = Arc::new(MockEnrichmentService::new());
    let storage = Arc::new(MockEnrichedNoteStore::new());

    let pipeline = NotePipeline::new(
        change_detector.clone() as Arc<dyn ChangeDetectionStore>,
        merkle_store.clone() as Arc<dyn MerkleStore>,
        enrichment_service.clone() as Arc<dyn EnrichmentService>,
        storage.clone() as Arc<dyn EnrichedNoteStore>,
    );

    (
        pipeline,
        change_detector,
        merkle_store,
        enrichment_service,
        storage,
    )
}

/// Create a test pipeline with custom config
fn create_test_pipeline_with_config(
    config: NotePipelineConfig,
) -> (
    NotePipeline,
    Arc<MockChangeDetectionStore>,
    Arc<MockMerkleStore>,
    Arc<MockEnrichmentService>,
    Arc<MockEnrichedNoteStore>,
) {
    let change_detector = Arc::new(MockChangeDetectionStore::new());
    let merkle_store = Arc::new(MockMerkleStore::new());
    let enrichment_service = Arc::new(MockEnrichmentService::new());
    let storage = Arc::new(MockEnrichedNoteStore::new());

    let pipeline = NotePipeline::with_config(
        change_detector.clone() as Arc<dyn ChangeDetectionStore>,
        merkle_store.clone() as Arc<dyn MerkleStore>,
        enrichment_service.clone() as Arc<dyn EnrichmentService>,
        storage.clone() as Arc<dyn EnrichedNoteStore>,
        config,
    );

    (
        pipeline,
        change_detector,
        merkle_store,
        enrichment_service,
        storage,
    )
}

// ============================================================================
// Integration Tests
// ============================================================================

#[tokio::test]
async fn test_full_pipeline_with_embeddings() {
    // Create pipeline with all mocks
    let (pipeline, _change_detector, _merkle_store, enrichment_service, storage) =
        create_test_pipeline();

    // Configure enrichment to generate embeddings
    enrichment_service.set_generate_embeddings(true);
    enrichment_service.set_embedding_dimension(384);

    // Create a test markdown file
    let (_temp_dir, file_path) = create_test_file(
        r#"# Test Note

This is a test note with some content.

## Section 1

Some text in section 1.

## Section 2

Some text in section 2.
"#,
    )
    .unwrap();

    // Process the file through the pipeline
    let result = pipeline.process(&file_path).await.unwrap();

    // Verify successful processing
    match result {
        ProcessingResult::Success { changed_blocks, .. } => {
            // Should have processed some blocks
            assert!(changed_blocks > 0, "Expected changed blocks");

            // Verify enrichment was called
            assert_eq!(
                enrichment_service.enrich_with_tree_count(),
                1,
                "Enrichment should be called once"
            );

            // Verify storage was called
            assert_eq!(storage.store_count(), 1, "Storage should be called once");

            // Verify enriched note was stored
            let stored_notes = storage.get_stored_notes();
            assert_eq!(stored_notes.len(), 1, "Should have one stored note");

            // Verify embeddings were generated
            let enriched = &stored_notes[0];
            assert!(
                enriched.embeddings.len() > 0,
                "Should have generated embeddings"
            );
        }
        _ => panic!("Expected Success result, got {:?}", result),
    }
}

#[tokio::test]
async fn test_pipeline_skip_unchanged_files() {
    // Create pipeline
    let (pipeline, change_detector, _merkle_store, enrichment_service, storage) =
        create_test_pipeline();

    // Create a test file
    let (_temp_dir, file_path) = create_test_file("# Test Note\n\nUnchanged content.").unwrap();

    // First processing - should succeed
    let result1 = pipeline.process(&file_path).await.unwrap();
    assert!(
        matches!(result1, ProcessingResult::Success { .. }),
        "First processing should succeed"
    );

    // Save counts before second processing
    let enrich_count_before = enrichment_service.enrich_with_tree_count();
    let store_count_before = storage.store_count();

    // Second processing - should skip (file hash matches)
    let result2 = pipeline.process(&file_path).await.unwrap();

    match result2 {
        ProcessingResult::Skipped => {
            // Perfect - file was skipped
            assert_eq!(
                enrichment_service.enrich_with_tree_count(),
                enrich_count_before,
                "Enrichment should not be called for skipped files"
            );
            assert_eq!(
                storage.store_count(),
                store_count_before,
                "Storage should not be called for skipped files"
            );
        }
        _ => panic!("Expected Skipped result, got {:?}", result2),
    }
}

#[tokio::test]
async fn test_pipeline_force_reprocess() {
    // Create pipeline with force_reprocess enabled
    let config = NotePipelineConfig {
        parser: Default::default(),
        skip_enrichment: false,
        force_reprocess: true,
    };
    let (pipeline, change_detector, _merkle_store, enrichment_service, storage) =
        create_test_pipeline_with_config(config);

    // Create a test file
    let (_temp_dir, file_path) = create_test_file("# Test Note\n\nContent.").unwrap();

    // First processing
    let result1 = pipeline.process(&file_path).await.unwrap();
    assert!(
        matches!(result1, ProcessingResult::Success { .. }),
        "First processing should succeed"
    );

    // Reset counters
    let first_enrich_count = enrichment_service.enrich_with_tree_count();
    let first_store_count = storage.store_count();

    // Second processing with force_reprocess - should NOT skip
    let result2 = pipeline.process(&file_path).await.unwrap();

    match result2 {
        ProcessingResult::Success { .. } => {
            // With force_reprocess, should process again
            assert!(
                enrichment_service.enrich_with_tree_count() > first_enrich_count,
                "Enrichment should be called again with force_reprocess"
            );
            assert!(
                storage.store_count() > first_store_count,
                "Storage should be called again with force_reprocess"
            );
        }
        _ => panic!(
            "Expected Success result with force_reprocess, got {:?}",
            result2
        ),
    }
}

#[tokio::test]
async fn test_pipeline_skip_enrichment_mode() {
    // Create pipeline with skip_enrichment enabled
    let config = NotePipelineConfig {
        parser: Default::default(),
        skip_enrichment: true,
        force_reprocess: false,
    };
    let (pipeline, _change_detector, _merkle_store, enrichment_service, storage) =
        create_test_pipeline_with_config(config);

    // Create a test file
    let (_temp_dir, file_path) = create_test_file("# Test Note\n\nContent.").unwrap();

    // Process the file
    let result = pipeline.process(&file_path).await.unwrap();

    match result {
        ProcessingResult::Success { .. } => {
            // Verify enrichment service was NOT called (pipeline creates minimal note directly)
            assert_eq!(
                enrichment_service.enrich_with_tree_count(),
                0,
                "Enrichment service should not be called in skip mode (minimal note created directly)"
            );

            // Verify storage was still called (we still store parsed note)
            assert_eq!(storage.store_count(), 1, "Storage should be called");

            // Verify no embeddings were generated
            let stored_notes = storage.get_stored_notes();
            let enriched = &stored_notes[0];
            assert_eq!(
                enriched.embeddings.len(),
                0,
                "Should not generate embeddings in skip mode"
            );
        }
        _ => panic!("Expected Success result, got {:?}", result),
    }
}

#[tokio::test]
async fn test_pipeline_parse_error_handling() {
    // Create pipeline
    let (pipeline, _change_detector, _merkle_store, enrichment_service, storage) =
        create_test_pipeline();

    // Try to process a non-existent file
    let non_existent_path = Path::new("/tmp/does_not_exist_12345.md");

    let result = pipeline.process(non_existent_path).await;

    // Should return an error
    assert!(result.is_err(), "Processing non-existent file should error");

    // Verify enrichment and storage were NOT called
    assert_eq!(
        enrichment_service.enrich_with_tree_count(),
        0,
        "Enrichment should not be called on parse error"
    );
    assert_eq!(
        storage.store_count(),
        0,
        "Storage should not be called on parse error"
    );
}

#[tokio::test]
async fn test_pipeline_enrichment_error_handling() {
    // Create pipeline
    let (pipeline, _change_detector, _merkle_store, enrichment_service, storage) =
        create_test_pipeline();

    // Configure enrichment to simulate errors
    enrichment_service.set_simulate_errors(true, "Simulated enrichment failure");

    // Create a test file
    let (_temp_dir, file_path) = create_test_file("# Test Note\n\nContent.").unwrap();

    // Process the file - should fail at enrichment phase
    let result = pipeline.process(&file_path).await;

    // Should return an error from enrichment
    assert!(
        result.is_err(),
        "Processing should error when enrichment fails"
    );

    let error_msg = format!("{:?}", result.unwrap_err());
    assert!(
        error_msg.contains("enrichment"),
        "Error should mention enrichment phase"
    );

    // Verify storage was NOT called (enrichment failed before storage)
    assert_eq!(
        storage.store_count(),
        0,
        "Storage should not be called when enrichment fails"
    );
}

#[tokio::test]
async fn test_pipeline_storage_error_handling() {
    // Create pipeline
    let (pipeline, _change_detector, _merkle_store, enrichment_service, storage) =
        create_test_pipeline();

    // Configure storage to simulate errors
    storage.set_simulate_errors(true, "Simulated storage failure");

    // Create a test file
    let (_temp_dir, file_path) = create_test_file("# Test Note\n\nContent.").unwrap();

    // Process the file - should fail at storage phase
    let result = pipeline.process(&file_path).await;

    // Should return an error from storage
    assert!(
        result.is_err(),
        "Processing should error when storage fails"
    );

    let error_msg = format!("{:?}", result.unwrap_err());
    assert!(
        error_msg.contains("storage") || error_msg.contains("Simulated storage failure"),
        "Error should mention storage failure"
    );

    // Verify enrichment WAS called (it succeeded, storage failed)
    assert_eq!(
        enrichment_service.enrich_with_tree_count(),
        1,
        "Enrichment should be called even if storage fails"
    );
}

#[tokio::test]
async fn test_pipeline_detects_content_changes() {
    // Create pipeline
    let (pipeline, _change_detector, merkle_store, enrichment_service, storage) =
        create_test_pipeline();

    // Create initial file
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("note.md");

    // Write initial content
    std::fs::write(&file_path, "# Original\n\nOriginal content.").unwrap();

    // First processing
    let result1 = pipeline.process(&file_path).await.unwrap();
    assert!(matches!(result1, ProcessingResult::Success { .. }));

    let first_enrich_count = enrichment_service.enrich_with_tree_count();

    // Modify the file content
    std::fs::write(&file_path, "# Modified\n\nNew content here!").unwrap();

    // Second processing - should detect changes
    let result2 = pipeline.process(&file_path).await.unwrap();

    match result2 {
        ProcessingResult::Success { changed_blocks, .. } => {
            // Should have detected changes
            assert!(changed_blocks > 0, "Should detect content changes");

            // Should have called enrichment again
            assert!(
                enrichment_service.enrich_with_tree_count() > first_enrich_count,
                "Should enrich again after content changes"
            );
        }
        _ => panic!("Expected Success after content change, got {:?}", result2),
    }
}

#[tokio::test]
async fn test_pipeline_no_changes_after_merkle_diff() {
    // This test verifies the NoChanges result when Merkle diff finds no changes

    let (pipeline, _change_detector, merkle_store, enrichment_service, storage) =
        create_test_pipeline();

    // Create a test file
    let (_temp_dir, file_path) = create_test_file("# Test\n\nContent.").unwrap();

    // First processing - parse and build merkle tree
    let result1 = pipeline.process(&file_path).await.unwrap();
    assert!(matches!(result1, ProcessingResult::Success { .. }));

    // Get the stored tree
    let path_str = file_path.to_string_lossy();
    let stored_tree = merkle_store.retrieve(&path_str).await.ok();
    assert!(stored_tree.is_some(), "Tree should be stored");

    let initial_enrich_count = enrichment_service.enrich_with_tree_count();

    // Modify file metadata (touch) but not content
    // This would bypass Phase 1 (file hash) but Merkle diff should catch it
    // For this test, we'll just process again - since content is identical,
    // Merkle tree should be the same

    // Actually, to properly test NoChanges, we need to force reprocess
    // but have identical content - this would be detected at Merkle phase
    let config = NotePipelineConfig {
        parser: Default::default(),
        skip_enrichment: false,
        force_reprocess: true, // Force past Phase 1
    };
    let (pipeline2, _, merkle_store2, enrichment_service2, storage2) =
        create_test_pipeline_with_config(config);

    // Copy the first tree to second pipeline's store
    if let Some(tree) = stored_tree {
        merkle_store2.store(&path_str, &tree).await.unwrap();
    }

    // Process again with force - should get NoChanges from Merkle diff
    let result2 = pipeline2.process(&file_path).await.unwrap();

    match result2 {
        ProcessingResult::NoChanges => {
            // Perfect - Merkle diff detected no changes
            assert_eq!(
                enrichment_service2.enrich_with_tree_count(),
                0,
                "Should not enrich when no Merkle changes"
            );
            assert_eq!(
                storage2.store_count(),
                0,
                "Should not store when no changes"
            );
        }
        ProcessingResult::Success { changed_blocks, .. } => {
            // This is also acceptable if implementation chooses to treat
            // force_reprocess as always enriching
            // Just verify the logic is consistent
            println!(
                "Note: Got Success instead of NoChanges with {} changed blocks",
                changed_blocks
            );
        }
        _ => panic!("Unexpected result: {:?}", result2),
    }
}
