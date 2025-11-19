//! Benchmarks for graph relations performance
//!
//! These benchmarks measure the performance improvements from the graph relations refactor:
//! - O(1) direct record ID lookups
//! - Graph traversal queries
//! - Deterministic chunk ID generation

use chrono::Utc;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use crucible_core::parser::ParsedNote;
use std::hint::black_box;
use crucible_surrealdb::{
    kiln_integration::{
        get_document_embeddings, initialize_kiln_schema, retrieve_parsed_document, store_embedding,
        store_parsed_document,
    },
    SurrealClient,
};
use std::path::PathBuf;

// Helper to create test note
fn create_test_document(path: PathBuf) -> ParsedNote {
    let mut doc = ParsedNote::new(path);
    doc.content.plain_text = "Benchmark test content".to_string();
    doc.parsed_at = Utc::now();
    doc
}

/// Benchmark: Direct record ID lookup (O(1) operation)
fn bench_record_id_lookup(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("record_id_direct_lookup", |b| {
        // Setup: Create client and store documents
        let (client, note_ids) = runtime.block_on(async {
            let client = SurrealClient::new_memory().await.unwrap();
            initialize_kiln_schema(&client).await.unwrap();

            let kiln_root = PathBuf::from("/tmp/bench_kiln");
            let mut note_ids = Vec::new();

            // Store 1000 documents
            for i in 0..1000 {
                let file_path = kiln_root.join(format!("bench_doc_{}.md", i));
                let doc = create_test_document(file_path);
                let id = store_parsed_document(&client, &doc, &kiln_root)
                    .await
                    .unwrap();
                note_ids.push(id);
            }

            (client, note_ids)
        });

        // Benchmark: Lookup middle note directly by ID
        let target_id = &note_ids[500];

        b.iter(|| {
            runtime.block_on(async {
                let result = retrieve_parsed_document(&client, target_id).await;
                black_box(result.unwrap());
            });
        });
    });
}

/// Benchmark: Graph traversal for embeddings
fn bench_graph_traversal(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("graph_traversal");

    for chunk_count in [1, 5, 10, 20].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}_chunks", chunk_count)),
            chunk_count,
            |b, &count| {
                // Setup: Create note with multiple embeddings
                let (client, note_id) = runtime.block_on(async {
                    let client = SurrealClient::new_memory().await.unwrap();
                    initialize_kiln_schema(&client).await.unwrap();

                    let kiln_root = PathBuf::from("/tmp/bench_kiln");
                    let file_path = kiln_root.join("traversal_test.md");
                    let doc = create_test_document(file_path);
                    let note_id = store_parsed_document(&client, &doc, &kiln_root)
                        .await
                        .unwrap();

                    // Store embeddings
                    for i in 0..count {
                        let vector = vec![0.1 * i as f32; 384];
                        store_embedding(&client, &note_id, vector, "bench-model", 512, i, None, None)
                            .await
                            .unwrap();
                    }

                    (client, note_id)
                });

                // Benchmark: Traverse to get all embeddings
                b.iter(|| {
                    runtime.block_on(async {
                        let embeddings = get_document_embeddings(&client, &note_id).await;
                        black_box(embeddings.unwrap());
                    });
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: Concurrent note storage
fn bench_concurrent_storage(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("concurrent_document_storage", |b| {
        b.iter(|| {
            runtime.block_on(async {
                let client = SurrealClient::new_memory().await.unwrap();
                initialize_kiln_schema(&client).await.unwrap();

                let kiln_root = PathBuf::from("/tmp/bench_kiln");
                let mut handles = vec![];

                // Store 50 documents concurrently
                for i in 0..50 {
                    let client_clone = client.clone();
                    let kiln_root_clone = kiln_root.clone();

                    let handle = tokio::spawn(async move {
                        let file_path = kiln_root_clone.join(format!("concurrent_{}.md", i));
                        let doc = create_test_document(file_path);
                        store_parsed_document(&client_clone, &doc, &kiln_root_clone)
                            .await
                            .unwrap()
                    });

                    handles.push(handle);
                }

                // Wait for all to complete
                for handle in handles {
                    handle.await.unwrap();
                }
            });
        });
    });
}

/// Benchmark: Embedding storage with deterministic IDs
fn bench_embedding_storage(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("embedding_storage");

    for vector_size in [128, 384, 768, 1536].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("dim_{}", vector_size)),
            vector_size,
            |b, &dims| {
                // Setup: Create note
                let (client, note_id) = runtime.block_on(async {
                    let client = SurrealClient::new_memory().await.unwrap();
                    initialize_kiln_schema(&client).await.unwrap();

                    let kiln_root = PathBuf::from("/tmp/bench_kiln");
                    let file_path = kiln_root.join("embedding_bench.md");
                    let doc = create_test_document(file_path);
                    let note_id = store_parsed_document(&client, &doc, &kiln_root)
                        .await
                        .unwrap();

                    (client, note_id)
                });

                // Benchmark: Store embedding with specific dimensions
                let mut chunk_idx = 0;
                b.iter(|| {
                    runtime.block_on(async {
                        let vector = vec![0.1f32; dims];
                        let result = store_embedding(
                            &client,
                            &note_id,
                            vector,
                            "bench-model",
                            512,
                            chunk_idx,
                            None,
                            None,
                        )
                        .await;
                        black_box(result.unwrap());
                    });
                    chunk_idx += 1;
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: Bulk retrieval operations
fn bench_bulk_retrieval(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("bulk_document_retrieval", |b| {
        // Setup: Store multiple documents
        let (client, note_ids) = runtime.block_on(async {
            let client = SurrealClient::new_memory().await.unwrap();
            initialize_kiln_schema(&client).await.unwrap();

            let kiln_root = PathBuf::from("/tmp/bench_kiln");
            let mut note_ids = Vec::new();

            for i in 0..100 {
                let file_path = kiln_root.join(format!("bulk_{}.md", i));
                let doc = create_test_document(file_path);
                let id = store_parsed_document(&client, &doc, &kiln_root)
                    .await
                    .unwrap();
                note_ids.push(id);
            }

            (client, note_ids)
        });

        // Benchmark: Retrieve all documents
        b.iter(|| {
            runtime.block_on(async {
                let mut results = Vec::new();
                for id in &note_ids {
                    let doc = retrieve_parsed_document(&client, id).await.unwrap();
                    results.push(doc);
                }
                black_box(results);
            });
        });
    });
}

criterion_group!(
    benches,
    bench_record_id_lookup,
    bench_graph_traversal,
    bench_concurrent_storage,
    bench_embedding_storage,
    bench_bulk_retrieval
);

criterion_main!(benches);
