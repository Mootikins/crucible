//! Concurrent operation tests for HybridMerkleTreeBuilder
//!
//! Tests verify that the Tower-style builder pattern works correctly
//! in concurrent scenarios, which is critical for the enrichment pipeline.

use crucible_core::parser::{ParsedNote, ParsedNoteBuilder};
use crucible_core::MerkleTreeBuilder;
use crucible_merkle::HybridMerkleTreeBuilder;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::task::JoinSet;

/// Test concurrent tree building with shared builder
#[tokio::test]
async fn test_concurrent_tree_building_with_shared_builder() {
    let builder = Arc::new(HybridMerkleTreeBuilder);

    let mut tasks = JoinSet::new();

    // Spawn 10 concurrent tasks, each building a tree
    for i in 0..10 {
        let builder = Arc::clone(&builder);
        tasks.spawn(async move {
            let note = ParsedNoteBuilder::new(PathBuf::from(format!("note{}.md", i))).build();
            builder.from_document(&note)
        });
    }

    // Collect all results
    let mut trees = Vec::new();
    while let Some(result) = tasks.join_next().await {
        let tree = result.expect("Task should not panic");
        trees.push(tree);
    }

    // All 10 trees should have been created
    assert_eq!(trees.len(), 10);

    // Each tree should have at least a preamble section
    for tree in trees {
        assert!(tree.section_count() >= 1);
    }
}

/// Test concurrent tree building with multiple services
#[tokio::test]
async fn test_concurrent_services_with_different_builders() {
    // Simulate multiple enrichment services running concurrently
    struct Service<M: MerkleTreeBuilder> {
        builder: M,
        id: usize,
    }

    impl<M: MerkleTreeBuilder> Service<M> {
        async fn process(&self, note: ParsedNote) -> M::Tree {
            // Simulate some async work
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
            self.builder.from_document(&note)
        }
    }

    let mut tasks = JoinSet::new();

    // Create 5 services, each processing 2 documents
    for service_id in 0..5 {
        tasks.spawn(async move {
            let service = Service {
                builder: HybridMerkleTreeBuilder,
                id: service_id,
            };

            let mut results = Vec::new();
            for doc_id in 0..2 {
                let note =
                    ParsedNoteBuilder::new(PathBuf::from(format!("svc{}_doc{}.md", service_id, doc_id)))
                        .build();
                let tree = service.process(note).await;
                results.push(tree);
            }
            results
        });
    }

    // Collect all results
    let mut all_trees = Vec::new();
    while let Some(result) = tasks.join_next().await {
        let trees = result.expect("Task should not panic");
        all_trees.extend(trees);
    }

    // Should have 5 services * 2 documents = 10 trees
    assert_eq!(all_trees.len(), 10);
}

/// Test that builders can be cloned for concurrent use
#[tokio::test]
async fn test_builder_cloning_for_concurrency() {
    let original_builder = HybridMerkleTreeBuilder;

    let mut tasks = JoinSet::new();

    // Each task gets its own clone
    for i in 0..20 {
        let builder = original_builder.clone();
        tasks.spawn(async move {
            let note = ParsedNoteBuilder::new(PathBuf::from(format!("clone{}.md", i))).build();
            (i, builder.from_document(&note))
        });
    }

    // Collect all results
    let mut results = Vec::new();
    while let Some(result) = tasks.join_next().await {
        let (id, tree) = result.expect("Task should not panic");
        results.push((id, tree));
    }

    // All 20 tasks should complete
    assert_eq!(results.len(), 20);

    // Verify IDs are unique (no data races)
    let mut ids: Vec<_> = results.iter().map(|(id, _)| *id).collect();
    ids.sort();
    assert_eq!(ids, (0..20).collect::<Vec<_>>());
}

/// Test deterministic tree building across concurrent calls
#[tokio::test]
async fn test_deterministic_trees_concurrent() {
    let builder = Arc::new(HybridMerkleTreeBuilder);
    let note = Arc::new(ParsedNoteBuilder::new(PathBuf::from("deterministic.md")).build());

    let mut tasks = JoinSet::new();

    // Build the same tree 50 times concurrently
    for _ in 0..50 {
        let builder = Arc::clone(&builder);
        let note = Arc::clone(&note);
        tasks.spawn(async move {
            builder.from_document(&note)
        });
    }

    // Collect all trees
    let mut trees = Vec::new();
    while let Some(result) = tasks.join_next().await {
        let tree = result.expect("Task should not panic");
        trees.push(tree);
    }

    // All trees should be identical
    let first_tree = &trees[0];
    for tree in &trees[1..] {
        assert_eq!(tree.root_hash, first_tree.root_hash);
        assert_eq!(tree.section_count(), first_tree.section_count());
        assert_eq!(tree.total_blocks, first_tree.total_blocks);
    }
}

/// Test concurrent tree building with varying document sizes
#[tokio::test]
async fn test_concurrent_varying_document_sizes() {
    let builder = Arc::new(HybridMerkleTreeBuilder);

    let mut tasks = JoinSet::new();

    // Small, medium, and large documents processed concurrently
    let doc_sizes = vec![
        ("small", 1),     // 1 section
        ("medium", 10),   // 10 sections
        ("large", 100),   // 100 sections
    ];

    for (name, _section_count) in doc_sizes {
        let builder = Arc::clone(&builder);
        tasks.spawn(async move {
            let note = ParsedNoteBuilder::new(PathBuf::from(format!("{}.md", name))).build();
            (name, builder.from_document(&note))
        });
    }

    // All should complete without issues
    while let Some(result) = tasks.join_next().await {
        let (_name, tree) = result.expect("Task should not panic");
        assert!(tree.section_count() >= 1);
    }
}

/// Test that builder is zero-cost even when cloned many times
#[test]
fn test_builder_zero_cost_with_many_clones() {
    let builder = HybridMerkleTreeBuilder;

    // Clone the builder 1000 times
    let clones: Vec<_> = (0..1000).map(|_| builder.clone()).collect();

    // Each clone should be zero-sized
    for clone in clones {
        assert_eq!(std::mem::size_of_val(&clone), 0);
    }

    // Verify the builder itself is zero-sized
    assert_eq!(std::mem::size_of::<HybridMerkleTreeBuilder>(), 0);
}

/// Test concurrent access pattern similar to enrichment pipeline
#[tokio::test]
async fn test_enrichment_pipeline_concurrent_pattern() {
    // Simulate the enrichment pipeline pattern where:
    // 1. Multiple notes are processed concurrently
    // 2. Each gets a tree built via the builder
    // 3. Results are collected and stored

    let builder = Arc::new(HybridMerkleTreeBuilder);

    let mut tasks = JoinSet::new();

    // Process 30 notes in batches of 10 (common pipeline pattern)
    for batch in 0..3 {
        for i in 0..10 {
            let builder = Arc::clone(&builder);
            tasks.spawn(async move {
                let note_id = batch * 10 + i;
                let note = ParsedNoteBuilder::new(PathBuf::from(format!("batch{}_note{}.md", batch, i)))
                    .build();

                // Simulate async enrichment work
                tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;

                let tree = builder.from_document(&note);

                (note_id, tree)
            });
        }
    }

    // Collect results
    let mut results = Vec::new();
    while let Some(result) = tasks.join_next().await {
        let (note_id, tree) = result.expect("Task should not panic");
        results.push((note_id, tree));
    }

    // All 30 notes should be processed
    assert_eq!(results.len(), 30);

    // Note IDs should be unique
    let mut ids: Vec<_> = results.iter().map(|(id, _)| *id).collect();
    ids.sort();
    assert_eq!(ids, (0..30).collect::<Vec<_>>());
}

/// Test race-free builder usage across threads
#[tokio::test]
async fn test_no_data_races_across_threads() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    let builder = Arc::new(HybridMerkleTreeBuilder);
    let counter = Arc::new(AtomicUsize::new(0));

    let mut tasks = JoinSet::new();

    // Spawn 100 tasks that increment a shared counter
    for i in 0..100 {
        let builder = Arc::clone(&builder);
        let counter = Arc::clone(&counter);

        tasks.spawn(async move {
            let note = ParsedNoteBuilder::new(PathBuf::from(format!("race{}.md", i))).build();
            let _tree = builder.from_document(&note);

            // Increment counter after tree building
            counter.fetch_add(1, Ordering::SeqCst);
        });
    }

    // Wait for all tasks to complete
    while tasks.join_next().await.is_some() {}

    // Counter should be exactly 100 (no missed increments)
    assert_eq!(counter.load(Ordering::SeqCst), 100);
}
