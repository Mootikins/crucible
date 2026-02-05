//! Write Throughput Comparison Benchmarks
//!
//! Compares write performance (insert, bulk insert, update, delete) across backends.
//!
//! Run with:
//! ```bash
//! cargo bench -p crucible-benchmarks --features sqlite,surrealdb -- write
//! ```

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
#[cfg(any(feature = "sqlite", feature = "surrealdb"))]
use crucible_benchmarks::fixtures::random_embedding;
#[cfg(any(feature = "sqlite", feature = "surrealdb"))]
use crucible_core::parser::BlockHash;
#[cfg(any(feature = "sqlite", feature = "surrealdb"))]
use crucible_core::storage::NoteRecord;
#[cfg(any(feature = "sqlite", feature = "surrealdb"))]
use crucible_core::storage::NoteStore;
#[cfg(any(feature = "sqlite", feature = "surrealdb"))]
use rand::prelude::*;
#[cfg(any(feature = "sqlite", feature = "surrealdb"))]
use rand::rngs::StdRng;
#[cfg(any(feature = "sqlite", feature = "surrealdb"))]
use rand::SeedableRng;
#[cfg(feature = "sqlite")]
use std::sync::Arc;
#[cfg(any(feature = "sqlite", feature = "surrealdb"))]
use tempfile::TempDir;

// =============================================================================
// Test Data Generation
// =============================================================================

/// Generate a single note for insert testing
#[cfg(any(feature = "sqlite", feature = "surrealdb"))]
fn generate_note(id: usize) -> NoteRecord {
    let mut rng = StdRng::seed_from_u64(id as u64);
    let hash_bytes: [u8; 32] = rng.random();

    NoteRecord::new(
        format!("notes/bench-{:05}.md", id),
        BlockHash::new(hash_bytes),
    )
    .with_title(format!("Benchmark Note {}", id))
    .with_tags(vec![format!("tag{}", id % 10), "benchmark".to_string()])
    .with_links(vec![
        format!("notes/link-{:05}.md", id % 100),
        format!("notes/link-{:05}.md", (id + 1) % 100),
    ])
    .with_embedding(random_embedding(&mut rng, 384))
}

/// Generate batch of notes for bulk insert testing
#[cfg(any(feature = "sqlite", feature = "surrealdb"))]
fn generate_batch(start_id: usize, count: usize) -> Vec<NoteRecord> {
    (start_id..start_id + count).map(generate_note).collect()
}

// =============================================================================
// SQLite Backend
// =============================================================================

#[cfg(feature = "sqlite")]
mod sqlite_bench {
    use super::*;
    use crucible_sqlite::{SqliteConfig, SqliteNoteStore, SqlitePool};

    pub struct SqliteFixture {
        pub note_store: Arc<SqliteNoteStore>,
        _pool: SqlitePool,
    }

    pub async fn setup(dir: &TempDir) -> SqliteFixture {
        let db_path = dir.path().join("bench.db");
        let config = SqliteConfig::new(db_path.to_string_lossy().as_ref());
        let pool = SqlitePool::new(config).expect("Failed to create SQLite pool");

        let note_store = crucible_sqlite::create_note_store(pool.clone())
            .await
            .expect("Failed to create SQLite store");

        SqliteFixture {
            note_store: Arc::new(note_store),
            _pool: pool,
        }
    }

    /// Setup with existing data for update/delete tests
    pub async fn setup_with_data(dir: &TempDir, note_count: usize) -> SqliteFixture {
        let fixture = setup(dir).await;

        // Insert initial data
        let notes = generate_batch(0, note_count);
        for note in notes {
            fixture.note_store.upsert(note).await.unwrap();
        }

        fixture
    }
}

// =============================================================================
// SurrealDB Backend
// =============================================================================

#[cfg(feature = "surrealdb")]
mod surreal_bench {
    use super::*;
    use crucible_surrealdb::{adapters, SurrealDbConfig};

    pub struct SurrealFixture {
        pub note_store: Arc<dyn NoteStore>,
    }

    pub async fn setup(_dir: &TempDir) -> SurrealFixture {
        // Use in-memory for write benchmarks (faster setup)
        let config = SurrealDbConfig {
            path: ":memory:".to_string(),
            namespace: "bench".to_string(),
            database: "notes".to_string(),
            max_connections: None,
            timeout_seconds: None,
        };

        let handle = adapters::create_surreal_client(config)
            .await
            .expect("Failed to create SurrealDB client");

        let note_store = handle.as_note_store();

        SurrealFixture { note_store }
    }

    /// Setup with existing data for update/delete tests
    pub async fn setup_with_data(dir: &TempDir, note_count: usize) -> SurrealFixture {
        let fixture = setup(dir).await;

        // Insert initial data
        let notes = generate_batch(0, note_count);
        for note in notes {
            fixture.note_store.upsert(note).await.unwrap();
        }

        fixture
    }
}

// =============================================================================
// Single Insert Benchmarks
// =============================================================================

#[allow(unused_variables, unused_mut)]
fn bench_single_insert(c: &mut Criterion) {
    #[cfg(any(feature = "sqlite", feature = "surrealdb"))]
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("write/single_insert");
    group.throughput(Throughput::Elements(1));

    #[cfg(feature = "sqlite")]
    {
        let dir = TempDir::new().unwrap();
        let fixture = rt.block_on(sqlite_bench::setup(&dir));
        let mut counter = 0usize;

        group.bench_function("sqlite", |b| {
            b.to_async(&rt).iter(|| {
                counter += 1;
                let note = generate_note(counter);
                let store = fixture.note_store.clone();
                async move { store.upsert(note).await.unwrap() }
            });
        });
    }

    #[cfg(feature = "surrealdb")]
    {
        let dir = TempDir::new().unwrap();
        let fixture = rt.block_on(surreal_bench::setup(&dir));
        let mut counter = 100_000usize; // Start at different offset

        group.bench_function("surrealdb", |b| {
            b.to_async(&rt).iter(|| {
                counter += 1;
                let note = generate_note(counter);
                let store = fixture.note_store.clone();
                async move { store.upsert(note).await.unwrap() }
            });
        });
    }

    group.finish();
}

// =============================================================================
// Bulk Insert Benchmarks
// =============================================================================

#[allow(unused_variables, unused_mut)]
fn bench_bulk_insert(c: &mut Criterion) {
    #[cfg(any(feature = "sqlite", feature = "surrealdb"))]
    let rt = tokio::runtime::Runtime::new().unwrap();

    for batch_size in [100, 500, 1000] {
        let mut group = c.benchmark_group(format!("write/bulk_insert_{}", batch_size));
        group.throughput(Throughput::Elements(batch_size as u64));
        group.sample_size(10); // Fewer samples for bulk operations

        #[cfg(feature = "sqlite")]
        {
            let mut batch_counter = 0usize;

            group.bench_function("sqlite", |b| {
                b.iter_custom(|iters| {
                    let mut total = std::time::Duration::ZERO;
                    for _ in 0..iters {
                        // Fresh database for each iteration
                        let dir = TempDir::new().unwrap();
                        let fixture = rt.block_on(sqlite_bench::setup(&dir));

                        batch_counter += 1;
                        let notes = generate_batch(batch_counter * batch_size, batch_size);

                        let start = std::time::Instant::now();
                        for note in notes {
                            rt.block_on(fixture.note_store.upsert(note)).unwrap();
                        }
                        total += start.elapsed();
                    }
                    total
                });
            });
        }

        #[cfg(feature = "surrealdb")]
        {
            let mut batch_counter = 0usize;

            group.bench_function("surrealdb", |b| {
                b.iter_custom(|iters| {
                    let mut total = std::time::Duration::ZERO;
                    for _ in 0..iters {
                        // Fresh database for each iteration
                        let dir = TempDir::new().unwrap();
                        let fixture = rt.block_on(surreal_bench::setup(&dir));

                        batch_counter += 1;
                        let notes =
                            generate_batch(batch_counter * batch_size + 100_000, batch_size);

                        let start = std::time::Instant::now();
                        for note in notes {
                            rt.block_on(fixture.note_store.upsert(note)).unwrap();
                        }
                        total += start.elapsed();
                    }
                    total
                });
            });
        }

        group.finish();
    }
}

// =============================================================================
// Update Benchmarks
// =============================================================================

/// Generate an updated version of a note (simulates content change)
#[cfg(any(feature = "sqlite", feature = "surrealdb"))]
fn generate_updated_note(id: usize, version: usize) -> NoteRecord {
    // Use combined seed for different content hash each version
    let mut rng = StdRng::seed_from_u64((id as u64) ^ ((version as u64) << 32));
    let hash_bytes: [u8; 32] = rng.random();

    NoteRecord::new(
        format!("notes/bench-{:05}.md", id),
        BlockHash::new(hash_bytes),
    )
    .with_title(format!("Benchmark Note {} (v{})", id, version))
    .with_tags(vec![
        format!("tag{}", id % 10),
        "benchmark".to_string(),
        format!("v{}", version),
    ])
    .with_links(vec![
        format!("notes/link-{:05}.md", id % 100),
        format!("notes/link-{:05}.md", (id + 1) % 100),
    ])
    .with_embedding(random_embedding(&mut rng, 384))
}

#[allow(unused_variables, unused_mut)]
fn bench_update(c: &mut Criterion) {
    #[cfg(any(feature = "sqlite", feature = "surrealdb"))]
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("write/update");
    group.throughput(Throughput::Elements(1));

    #[cfg(feature = "sqlite")]
    {
        let dir = TempDir::new().unwrap();
        let fixture = rt.block_on(sqlite_bench::setup_with_data(&dir, 1000));
        let mut counter = 0usize;
        let mut version = 0usize;

        group.bench_function("sqlite", |b| {
            b.to_async(&rt).iter(|| {
                counter = (counter + 1) % 1000;
                version += 1;
                let note = generate_updated_note(counter, version);
                let store = fixture.note_store.clone();
                async move { store.upsert(note).await.unwrap() }
            });
        });
    }

    #[cfg(feature = "surrealdb")]
    {
        let dir = TempDir::new().unwrap();
        let fixture = rt.block_on(surreal_bench::setup_with_data(&dir, 1000));
        let mut counter = 0usize;
        let mut version = 0usize;

        group.bench_function("surrealdb", |b| {
            b.to_async(&rt).iter(|| {
                counter = (counter + 1) % 1000;
                version += 1;
                let note = generate_updated_note(counter, version);
                let store = fixture.note_store.clone();
                async move { store.upsert(note).await.unwrap() }
            });
        });
    }

    group.finish();
}

// =============================================================================
// Delete Benchmarks
// =============================================================================

#[allow(unused_variables, unused_mut)]
fn bench_delete(c: &mut Criterion) {
    #[cfg(any(feature = "sqlite", feature = "surrealdb"))]
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("write/delete");
    group.throughput(Throughput::Elements(1));
    group.sample_size(10); // Fewer samples since we need fresh data each time

    #[cfg(feature = "sqlite")]
    {
        group.bench_function("sqlite", |b| {
            b.iter_custom(|iters| {
                let mut total = std::time::Duration::ZERO;
                for i in 0..iters {
                    // Fresh database with enough notes
                    let dir = TempDir::new().unwrap();
                    let fixture = rt.block_on(sqlite_bench::setup_with_data(&dir, 100));

                    let path = format!("notes/bench-{:05}.md", i % 100);
                    let start = std::time::Instant::now();
                    rt.block_on(fixture.note_store.delete(&path)).unwrap();
                    total += start.elapsed();
                }
                total
            });
        });
    }

    #[cfg(feature = "surrealdb")]
    {
        group.bench_function("surrealdb", |b| {
            b.iter_custom(|iters| {
                let mut total = std::time::Duration::ZERO;
                for i in 0..iters {
                    // Fresh database with enough notes
                    let dir = TempDir::new().unwrap();
                    let fixture = rt.block_on(surreal_bench::setup_with_data(&dir, 100));

                    let path = format!("notes/bench-{:05}.md", i % 100);
                    let start = std::time::Instant::now();
                    rt.block_on(fixture.note_store.delete(&path)).unwrap();
                    total += start.elapsed();
                }
                total
            });
        });
    }

    group.finish();
}

// =============================================================================
// Mixed Workload Benchmark (90% read, 10% write)
// =============================================================================

#[allow(unused_variables, unused_mut)]
fn bench_mixed_workload(c: &mut Criterion) {
    #[cfg(any(feature = "sqlite", feature = "surrealdb"))]
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("write/mixed_90_10");
    group.throughput(Throughput::Elements(100)); // 100 operations per iteration
    group.sample_size(20);

    #[cfg(feature = "sqlite")]
    {
        let dir = TempDir::new().unwrap();
        let fixture = rt.block_on(sqlite_bench::setup_with_data(&dir, 1000));
        let mut op_counter = 0usize;

        group.bench_function("sqlite", |b| {
            b.to_async(&rt).iter(|| {
                let store = fixture.note_store.clone();
                async move {
                    for i in 0..100 {
                        op_counter += 1;
                        if i % 10 == 0 {
                            // 10% writes
                            let note = generate_note(op_counter % 1000);
                            store.upsert(note).await.unwrap();
                        } else {
                            // 90% reads
                            let path = format!("notes/bench-{:05}.md", op_counter % 1000);
                            let _ = store.get(&path).await;
                        }
                    }
                }
            });
        });
    }

    #[cfg(feature = "surrealdb")]
    {
        let dir = TempDir::new().unwrap();
        let fixture = rt.block_on(surreal_bench::setup_with_data(&dir, 1000));
        let mut op_counter = 0usize;

        group.bench_function("surrealdb", |b| {
            b.to_async(&rt).iter(|| {
                let store = fixture.note_store.clone();
                async move {
                    for i in 0..100 {
                        op_counter += 1;
                        if i % 10 == 0 {
                            // 10% writes
                            let note = generate_note(op_counter % 1000);
                            store.upsert(note).await.unwrap();
                        } else {
                            // 90% reads
                            let path = format!("notes/bench-{:05}.md", op_counter % 1000);
                            let _ = store.get(&path).await;
                        }
                    }
                }
            });
        });
    }

    group.finish();
}

// =============================================================================
// Benchmark Registration
// =============================================================================

criterion_group!(
    write_benchmarks,
    bench_single_insert,
    bench_bulk_insert,
    bench_update,
    bench_delete,
    bench_mixed_workload
);
criterion_main!(write_benchmarks);
