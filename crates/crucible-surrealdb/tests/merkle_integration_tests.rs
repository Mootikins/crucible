//! Integration tests for Merkle tree persistence and DocumentIngestor integration
//!
//! These tests verify the complete pipeline:
//! - Document parsing → Tree building → Persistence → Retrieval → Verification
//! - NoteIngestor integration with automatic tree persistence
//! - Incremental updates with change detection
//! - Large documents with virtualization

use crucible_core::merkle::{HybridMerkleTree, VirtualizationConfig};
use crucible_core::parser::ParsedNote;
use crucible_parser::types::{Heading, NoteContent, Paragraph};
use crucible_surrealdb::eav_graph::{EAVGraphStore, NoteIngestor};
use crucible_surrealdb::{MerklePersistence, SurrealClient, SurrealDbConfig};
use std::path::PathBuf;

/// Create a test SurrealDB client with in-memory storage
async fn create_test_client() -> SurrealClient {
    let config = SurrealDbConfig {
        path: ":memory:".to_string(),
        ..Default::default()
    };
    SurrealClient::new(config)
        .await
        .expect("Failed to create test client")
}

/// Create a realistic test document with multiple sections
fn create_complex_document() -> ParsedNote {
    let mut doc = ParsedNote::default();
    doc.path = PathBuf::from("test_docs/complex_doc.md");
    doc.content = NoteContent::default();

    let mut offset = 0;

    // Add multiple sections with hierarchical headings
    let sections = vec![
        (
            "Introduction",
            1,
            "This is the introduction to the document.",
        ),
        ("Background", 1, "Background information goes here."),
        ("Technical Details", 2, "Detailed technical content."),
        ("Implementation", 2, "Implementation specifics."),
        ("Results", 1, "Results from experiments."),
        ("Analysis", 2, "Analysis of the results."),
        ("Conclusion", 1, "Final conclusions."),
    ];

    for (heading_text, level, content) in sections {
        // Add heading
        doc.content.headings.push(Heading {
            level,
            text: heading_text.to_string(),
            offset,
            id: Some(heading_text.to_lowercase().replace(' ', "-")),
        });
        offset += heading_text.len() + (level as usize) + 2; // Account for # markers and newline

        // Add content paragraph
        doc.content
            .paragraphs
            .push(Paragraph::new(content.to_string(), offset));
        offset += content.len() + 2; // Account for paragraph and newline
    }

    doc
}

/// Create a large document that triggers virtualization
fn create_large_document(section_count: usize) -> ParsedNote {
    let mut doc = ParsedNote::default();
    doc.path = PathBuf::from("test_docs/large_doc.md");
    doc.content = NoteContent::default();

    let mut offset = 0;

    for i in 0..section_count {
        let heading_text = format!("Section {}", i);
        let content_text = format!("This is the content for section {}.", i);

        doc.content.headings.push(Heading {
            level: 1,
            text: heading_text.clone(),
            offset,
            id: Some(format!("section-{}", i)),
        });
        offset += heading_text.len() + 3; // # + space + newline

        doc.content
            .paragraphs
            .push(Paragraph::new(content_text.clone(), offset));
        offset += content_text.len() + 2;
    }

    doc
}

#[tokio::test]
async fn test_end_to_end_pipeline() {
    let client = create_test_client().await;
    let persistence = MerklePersistence::new(client);

    // 1. Create and parse a complex document
    let doc = create_complex_document();
    let original_path = doc.path.to_string_lossy().to_string();

    // 2. Build Merkle tree
    let tree = HybridMerkleTree::from_document(&doc);
    assert_eq!(tree.sections.len(), 8); // root + 7 sections
    assert!(
        !tree.is_virtualized,
        "Should not be virtualized with only 8 sections"
    );

    // 3. Persist the tree
    persistence
        .store_tree(&original_path, &tree)
        .await
        .expect("Failed to store tree");

    // 4. Retrieve the tree
    let retrieved_tree = persistence
        .retrieve_tree(&original_path)
        .await
        .expect("Failed to retrieve tree");

    // 5. Verify integrity
    assert_eq!(
        tree.root_hash, retrieved_tree.root_hash,
        "Root hashes should match"
    );
    assert_eq!(
        tree.sections.len(),
        retrieved_tree.sections.len(),
        "Section count should match"
    );
    assert_eq!(
        tree.total_blocks, retrieved_tree.total_blocks,
        "Total blocks should match"
    );
    assert_eq!(
        tree.is_virtualized, retrieved_tree.is_virtualized,
        "Virtualization state should match"
    );

    // 6. Verify each section hash matches
    for (i, (original, retrieved)) in tree
        .sections
        .iter()
        .zip(retrieved_tree.sections.iter())
        .enumerate()
    {
        assert_eq!(
            original.binary_tree.root_hash, retrieved.binary_tree.root_hash,
            "Section {} hash should match",
            i
        );
        assert_eq!(
            original.heading, retrieved.heading,
            "Section {} heading should match",
            i
        );
        assert_eq!(
            original.depth, retrieved.depth,
            "Section {} depth should match",
            i
        );
    }

    println!("✅ End-to-end pipeline test passed!");
}

#[tokio::test]
async fn test_incremental_update_with_content_changes() {
    let client = create_test_client().await;
    let persistence = MerklePersistence::new(client);

    // 1. Create and store initial document
    let doc1 = create_complex_document();
    let tree_id = doc1.path.to_string_lossy().to_string();
    let tree1 = HybridMerkleTree::from_document(&doc1);

    persistence.store_tree(&tree_id, &tree1).await.unwrap();

    // 2. Modify the document (change one section's content)
    let mut doc2 = create_complex_document();
    // Modify the third paragraph (Technical Details section)
    doc2.content.paragraphs[2] = Paragraph::new(
        "MODIFIED: This is completely different technical content.".to_string(),
        doc2.content.paragraphs[2].offset,
    );

    let tree2 = HybridMerkleTree::from_document(&doc2);

    // 3. Verify root hash changed
    assert_ne!(
        tree1.root_hash, tree2.root_hash,
        "Root hash should change when content changes"
    );

    // 4. Detect which sections changed
    let diff = tree2.diff(&tree1);
    assert!(
        diff.root_hash_changed,
        "Diff should detect root hash change"
    );
    assert!(
        !diff.changed_sections.is_empty(),
        "Should detect changed sections"
    );

    // 5. Update incrementally
    let changed_indices: Vec<usize> = diff
        .changed_sections
        .iter()
        .map(|change| change.section_index)
        .collect();

    persistence
        .update_tree_incremental(&tree_id, &tree2, &changed_indices)
        .await
        .expect("Incremental update should succeed");

    // 6. Retrieve and verify
    let retrieved = persistence.retrieve_tree(&tree_id).await.unwrap();
    assert_eq!(
        tree2.root_hash, retrieved.root_hash,
        "Updated tree should match"
    );

    // 7. Verify unchanged sections still match original
    for (i, section) in tree2.sections.iter().enumerate() {
        if !changed_indices.contains(&i) {
            assert_eq!(
                section.binary_tree.root_hash, tree1.sections[i].binary_tree.root_hash,
                "Unchanged section {} should have same hash",
                i
            );
        }
    }

    println!("✅ Incremental update test passed!");
    println!("   Changed sections: {:?}", changed_indices);
}

#[tokio::test]
async fn test_large_document_virtualization() {
    let client = create_test_client().await;
    let persistence = MerklePersistence::new(client);

    // 1. Create a large document that triggers virtualization (>100 sections)
    let doc = create_large_document(150);
    let tree_id = doc.path.to_string_lossy().to_string();

    // 2. Build tree with auto-virtualization
    let config = VirtualizationConfig::default(); // threshold = 100
    let tree = HybridMerkleTree::from_document_with_config(&doc, &config);

    // 3. Verify virtualization occurred
    assert!(
        tree.is_virtualized,
        "Should be virtualized with 150 sections"
    );
    assert!(
        tree.virtual_sections.is_some(),
        "Should have virtual sections"
    );

    let virtual_sections = tree.virtual_sections.as_ref().unwrap();
    assert!(!virtual_sections.is_empty(), "Should have virtual sections");

    println!(
        "   Tree has {} sections, {} virtual sections",
        tree.sections.len(),
        virtual_sections.len()
    );

    // 4. Persist the virtualized tree
    persistence
        .store_tree(&tree_id, &tree)
        .await
        .expect("Should persist virtualized tree");

    // 5. Retrieve and verify
    let retrieved = persistence
        .retrieve_tree(&tree_id)
        .await
        .expect("Should retrieve virtualized tree");

    assert_eq!(
        tree.root_hash, retrieved.root_hash,
        "Root hash should match"
    );
    assert_eq!(
        tree.is_virtualized, retrieved.is_virtualized,
        "Virtualization state should match"
    );
    assert_eq!(
        tree.sections.len(),
        retrieved.sections.len(),
        "Section count should match"
    );

    // 6. Verify virtual sections were persisted correctly
    assert!(
        retrieved.virtual_sections.is_some(),
        "Retrieved tree should have virtual sections"
    );
    let retrieved_virtual = retrieved.virtual_sections.as_ref().unwrap();
    assert_eq!(
        virtual_sections.len(),
        retrieved_virtual.len(),
        "Virtual section count should match"
    );

    for (i, (original, retrieved)) in virtual_sections
        .iter()
        .zip(retrieved_virtual.iter())
        .enumerate()
    {
        assert_eq!(
            original.hash, retrieved.hash,
            "Virtual section {} hash should match",
            i
        );
        assert_eq!(
            original.section_count, retrieved.section_count,
            "Virtual section {} count should match",
            i
        );
    }

    println!("✅ Large document virtualization test passed!");
}

#[tokio::test]
async fn test_note_ingestor_integration() {
    let client = create_test_client().await;
    let store = EAVGraphStore::new(client.clone());
    let persistence = MerklePersistence::new(client);

    // 1. Create ingestor WITH Merkle persistence enabled
    let ingestor = NoteIngestor::with_merkle_persistence(&store, persistence.clone());

    // 2. Ingest a document
    let doc = create_complex_document();
    let doc_path = doc.path.to_string_lossy().to_string();

    ingestor
        .ingest(&doc, &doc_path)
        .await
        .expect("Document ingestion should succeed");

    // 3. Verify tree was automatically persisted
    let tree_metadata = persistence
        .get_tree_metadata(&doc_path)
        .await
        .expect("Should retrieve tree metadata")
        .expect("Tree should exist");

    assert_eq!(
        tree_metadata.id, doc_path,
        "Tree ID should match document path"
    );
    assert!(
        !tree_metadata.is_virtualized,
        "Small document shouldn't be virtualized"
    );

    // 4. Verify we can retrieve the full tree
    let tree = persistence
        .retrieve_tree(&doc_path)
        .await
        .expect("Should retrieve full tree");

    assert_eq!(tree.sections.len(), 8, "Should have 8 sections (root + 7)");

    // 5. Update the document
    let mut updated_doc = create_complex_document();
    updated_doc.content.paragraphs[0] = Paragraph::new(
        "UPDATED: This introduction has been modified.".to_string(),
        updated_doc.content.paragraphs[0].offset,
    );

    // 6. Ingest updated document (should trigger incremental update)
    ingestor
        .ingest(&updated_doc, &doc_path)
        .await
        .expect("Updated document ingestion should succeed");

    // 7. Verify tree was updated
    let updated_tree = persistence
        .retrieve_tree(&doc_path)
        .await
        .expect("Should retrieve updated tree");

    assert_ne!(
        tree.root_hash, updated_tree.root_hash,
        "Root hash should change after update"
    );

    println!("✅ NoteIngestor integration test passed!");
}

#[tokio::test]
async fn test_concurrent_tree_operations() {
    let client = create_test_client().await;
    let persistence = MerklePersistence::new(client);

    // Create multiple different documents
    let docs: Vec<_> = (0..5)
        .map(|i| {
            let mut doc = create_complex_document();
            doc.path = PathBuf::from(format!("test_docs/doc_{}.md", i));
            doc
        })
        .collect();

    // Store all trees concurrently
    let store_futures: Vec<_> = docs
        .iter()
        .map(|doc| {
            let tree = HybridMerkleTree::from_document(doc);
            let path = doc.path.to_string_lossy().to_string();
            let persistence = persistence.clone();

            async move { persistence.store_tree(&path, &tree).await }
        })
        .collect();

    // Execute all stores concurrently
    let results = futures::future::join_all(store_futures).await;

    // Verify all succeeded
    for (i, result) in results.iter().enumerate() {
        assert!(result.is_ok(), "Store operation {} should succeed", i);
    }

    // Retrieve all trees concurrently
    let retrieve_futures: Vec<_> = docs
        .iter()
        .map(|doc| {
            let path = doc.path.to_string_lossy().to_string();
            let persistence = persistence.clone();

            async move { persistence.retrieve_tree(&path).await }
        })
        .collect();

    let retrieved_results = futures::future::join_all(retrieve_futures).await;

    // Verify all retrieved successfully
    for (i, result) in retrieved_results.iter().enumerate() {
        assert!(result.is_ok(), "Retrieve operation {} should succeed", i);
    }

    // List all trees
    let all_trees = persistence.list_trees().await.expect("Should list trees");
    assert!(all_trees.len() >= 5, "Should have at least 5 trees");

    println!("✅ Concurrent operations test passed!");
    println!("   Stored and retrieved {} trees concurrently", docs.len());
}

#[tokio::test]
async fn test_persistence_error_handling() {
    let client = create_test_client().await;
    let persistence = MerklePersistence::new(client);

    // Test 1: Retrieve non-existent tree
    let result = persistence.retrieve_tree("nonexistent_tree").await;
    assert!(result.is_err(), "Should error on non-existent tree");

    // Test 2: Update with invalid indices
    let doc = create_complex_document();
    let tree = HybridMerkleTree::from_document(&doc);

    let result = persistence
        .update_tree_incremental("test_tree", &tree, &[999, 1000])
        .await;

    assert!(result.is_err(), "Should error on invalid indices");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Invalid section index"),
        "Error should mention invalid index"
    );

    // Test 3: Metadata for non-existent tree
    let metadata = persistence
        .get_tree_metadata("another_nonexistent")
        .await
        .expect("Should not error");

    assert!(
        metadata.is_none(),
        "Should return None for non-existent tree"
    );

    println!("✅ Error handling test passed!");
}

#[tokio::test]
async fn test_tree_deletion_cleanup() {
    let client = create_test_client().await;
    let persistence = MerklePersistence::new(client);

    // 1. Store a tree
    let doc = create_complex_document();
    let tree_id = doc.path.to_string_lossy().to_string();
    let tree = HybridMerkleTree::from_document(&doc);

    persistence.store_tree(&tree_id, &tree).await.unwrap();

    // 2. Verify it exists
    let metadata = persistence.get_tree_metadata(&tree_id).await.unwrap();
    assert!(metadata.is_some(), "Tree should exist");

    // 3. Delete the tree
    persistence
        .delete_tree(&tree_id)
        .await
        .expect("Deletion should succeed");

    // 4. Verify it's gone
    let metadata_after = persistence.get_tree_metadata(&tree_id).await.unwrap();
    assert!(metadata_after.is_none(), "Tree should be deleted");

    // 5. Verify retrieval fails
    let retrieve_result = persistence.retrieve_tree(&tree_id).await;
    assert!(
        retrieve_result.is_err(),
        "Should error when retrieving deleted tree"
    );

    println!("✅ Deletion cleanup test passed!");
}
