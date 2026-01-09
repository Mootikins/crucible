//! Storage Backend Comparison Benchmarks
//!
//! Compares performance of SQLite, LanceDB, and SurrealDB backends
//! for common NoteStore operations.
//!
//! Run with:
//! ```bash
//! # All backends
//! cargo bench -p crucible-storage-tests --features sqlite,lance,surrealdb
//!
//! # Individual backends
//! cargo bench -p crucible-storage-tests --features sqlite
//! cargo bench -p crucible-storage-tests --features lance
//! cargo bench -p crucible-storage-tests --features surrealdb
//! ```

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
#[cfg(any(feature = "sqlite", feature = "lance", feature = "surrealdb"))]
use criterion::BenchmarkId;
#[cfg(any(feature = "sqlite", feature = "lance", feature = "surrealdb"))]
use crucible_core::parser::BlockHash;
#[cfg(any(feature = "sqlite", feature = "lance", feature = "surrealdb"))]
use crucible_core::storage::NoteStore;
#[cfg(any(feature = "sqlite", feature = "lance", feature = "surrealdb"))]
use crucible_core::storage::NoteRecord;
#[cfg(any(feature = "sqlite", feature = "lance", feature = "surrealdb"))]
use rand::Rng;
#[cfg(any(feature = "sqlite", feature = "lance", feature = "surrealdb"))]
use tempfile::TempDir;

#[cfg(any(feature = "sqlite", feature = "lance", feature = "surrealdb"))]
const EMBEDDING_DIM: usize = 384; // Small model dimensions for faster benchmarks

/// Generate a random embedding vector
#[cfg(any(feature = "sqlite", feature = "lance", feature = "surrealdb"))]
fn random_embedding(dim: usize) -> Vec<f32> {
    let mut rng = rand::rng();
    (0..dim).map(|_| rng.random::<f32>()).collect()
}

/// Generate test note records
#[cfg(any(feature = "sqlite", feature = "lance", feature = "surrealdb"))]
fn generate_notes(count: usize) -> Vec<NoteRecord> {
    (0..count)
        .map(|i| {
            let path = format!("notes/test-note-{}.md", i);
            let hash_bytes: [u8; 32] = rand::rng().random();
            NoteRecord::new(path, BlockHash::new(hash_bytes))
                .with_title(format!("Test Note {}", i))
                .with_tags(vec![
                    format!("tag-{}", i % 10),
                    format!("category-{}", i % 5),
                ])
                .with_links(vec![
                    format!("notes/link-{}.md", (i + 1) % count),
                    format!("notes/link-{}.md", (i + 2) % count),
                ])
                .with_embedding(random_embedding(EMBEDDING_DIM))
        })
        .collect()
}

// =============================================================================
// SQLite Backend
// =============================================================================

#[cfg(feature = "sqlite")]
mod sqlite_bench {
    use super::*;
    use crucible_sqlite::{SqliteConfig, SqlitePool};

    pub async fn create_store(dir: &TempDir) -> impl NoteStore {
        let db_path = dir.path().join("bench.db");
        let config = SqliteConfig::new(db_path.to_string_lossy().as_ref());
        let pool = SqlitePool::new(config).expect("Failed to create SQLite pool");
        crucible_sqlite::create_note_store(pool)
            .await
            .expect("Failed to create SQLite store")
    }
}

// =============================================================================
// LanceDB Backend
// =============================================================================

#[cfg(feature = "lance")]
mod lance_bench {
    use super::*;

    pub async fn create_store(dir: &TempDir) -> impl NoteStore {
        let db_path = dir.path().join("bench.lance");
        crucible_lance::create_note_store_with_dimensions(
            db_path.to_string_lossy().as_ref(),
            EMBEDDING_DIM,
        )
        .await
        .expect("Failed to create Lance store")
    }
}

// =============================================================================
// SurrealDB Backend
// =============================================================================

#[cfg(feature = "surrealdb")]
mod surreal_bench {
    use super::*;
    use crucible_surrealdb::test_utils::SurrealClient;
    use crucible_surrealdb::SurrealDbConfig;

    pub async fn create_store(_dir: &TempDir) -> impl NoteStore {
        // SurrealDB uses in-memory for benchmarks (RocksDB file-based is slower to init)
        let config = SurrealDbConfig {
            path: ":memory:".to_string(),
            namespace: "bench".to_string(),
            database: "notes".to_string(),
            max_connections: None,
            timeout_seconds: None,
        };
        let client = SurrealClient::new(config)
            .await
            .expect("Failed to create SurrealDB client");
        crucible_surrealdb::test_utils::create_note_store_with_dimensions(client, EMBEDDING_DIM)
            .await
            .expect("Failed to create SurrealDB store")
    }
}

// =============================================================================
// Benchmark Functions
// =============================================================================

#[allow(unused_variables, unused_mut)]
fn bench_upsert(c: &mut Criterion) {
    #[cfg(any(feature = "sqlite", feature = "lance", feature = "surrealdb"))]
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("upsert");

    for count in [10, 100, 500] {
        group.throughput(Throughput::Elements(count as u64));
        #[cfg(any(feature = "sqlite", feature = "lance", feature = "surrealdb"))]
        let notes = generate_notes(count);

        #[cfg(feature = "sqlite")]
        {
            let dir = TempDir::new().unwrap();
            let store = rt.block_on(sqlite_bench::create_store(&dir));
            group.bench_with_input(BenchmarkId::new("sqlite", count), &notes, |b, notes| {
                b.to_async(&rt).iter(|| async {
                    for note in notes {
                        store.upsert(note.clone()).await.unwrap();
                    }
                });
            });
        }

        #[cfg(feature = "lance")]
        {
            let dir = TempDir::new().unwrap();
            let store = rt.block_on(lance_bench::create_store(&dir));
            group.bench_with_input(BenchmarkId::new("lance", count), &notes, |b, notes| {
                b.to_async(&rt).iter(|| async {
                    for note in notes {
                        store.upsert(note.clone()).await.unwrap();
                    }
                });
            });
        }

        #[cfg(feature = "surrealdb")]
        {
            let dir = TempDir::new().unwrap();
            let store = rt.block_on(surreal_bench::create_store(&dir));
            group.bench_with_input(BenchmarkId::new("surrealdb", count), &notes, |b, notes| {
                b.to_async(&rt).iter(|| async {
                    for note in notes {
                        store.upsert(note.clone()).await.unwrap();
                    }
                });
            });
        }
    }
    group.finish();
}

#[allow(unused_variables, unused_mut)]
fn bench_get(c: &mut Criterion) {
    #[cfg(any(feature = "sqlite", feature = "lance", feature = "surrealdb"))]
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("get");

    #[cfg(any(feature = "sqlite", feature = "lance", feature = "surrealdb"))]
    let notes = generate_notes(100);

    #[cfg(feature = "sqlite")]
    {
        let dir = TempDir::new().unwrap();
        let store = rt.block_on(async {
            let s = sqlite_bench::create_store(&dir).await;
            for note in &notes {
                s.upsert(note.clone()).await.unwrap();
            }
            s
        });
        group.bench_function("sqlite", |b| {
            b.to_async(&rt).iter(|| async {
                for note in &notes {
                    let _ = store.get(&note.path).await.unwrap();
                }
            });
        });
    }

    #[cfg(feature = "lance")]
    {
        let dir = TempDir::new().unwrap();
        let store = rt.block_on(async {
            let s = lance_bench::create_store(&dir).await;
            for note in &notes {
                s.upsert(note.clone()).await.unwrap();
            }
            s
        });
        group.bench_function("lance", |b| {
            b.to_async(&rt).iter(|| async {
                for note in &notes {
                    let _ = store.get(&note.path).await.unwrap();
                }
            });
        });
    }

    #[cfg(feature = "surrealdb")]
    {
        let dir = TempDir::new().unwrap();
        let store = rt.block_on(async {
            let s = surreal_bench::create_store(&dir).await;
            for note in &notes {
                s.upsert(note.clone()).await.unwrap();
            }
            s
        });
        group.bench_function("surrealdb", |b| {
            b.to_async(&rt).iter(|| async {
                for note in &notes {
                    let _ = store.get(&note.path).await.unwrap();
                }
            });
        });
    }

    group.finish();
}

#[allow(unused_variables, unused_mut)]
fn bench_list(c: &mut Criterion) {
    #[cfg(any(feature = "sqlite", feature = "lance", feature = "surrealdb"))]
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("list");

    for count in [100, 500, 1000] {
        #[cfg(any(feature = "sqlite", feature = "lance", feature = "surrealdb"))]
        let notes = generate_notes(count);

        #[cfg(feature = "sqlite")]
        {
            let dir = TempDir::new().unwrap();
            let store = rt.block_on(async {
                let s = sqlite_bench::create_store(&dir).await;
                for note in &notes {
                    s.upsert(note.clone()).await.unwrap();
                }
                s
            });
            group.bench_with_input(BenchmarkId::new("sqlite", count), &count, |b, _| {
                b.to_async(&rt).iter(|| async {
                    let _ = store.list().await.unwrap();
                });
            });
        }

        #[cfg(feature = "lance")]
        {
            let dir = TempDir::new().unwrap();
            let store = rt.block_on(async {
                let s = lance_bench::create_store(&dir).await;
                for note in &notes {
                    s.upsert(note.clone()).await.unwrap();
                }
                s
            });
            group.bench_with_input(BenchmarkId::new("lance", count), &count, |b, _| {
                b.to_async(&rt).iter(|| async {
                    let _ = store.list().await.unwrap();
                });
            });
        }

        #[cfg(feature = "surrealdb")]
        {
            let dir = TempDir::new().unwrap();
            let store = rt.block_on(async {
                let s = surreal_bench::create_store(&dir).await;
                for note in &notes {
                    s.upsert(note.clone()).await.unwrap();
                }
                s
            });
            group.bench_with_input(BenchmarkId::new("surrealdb", count), &count, |b, _| {
                b.to_async(&rt).iter(|| async {
                    let _ = store.list().await.unwrap();
                });
            });
        }
    }
    group.finish();
}

#[allow(unused_variables, unused_mut)]
fn bench_vector_search(c: &mut Criterion) {
    #[cfg(any(feature = "sqlite", feature = "lance", feature = "surrealdb"))]
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("vector_search");

    #[cfg(any(feature = "sqlite", feature = "lance", feature = "surrealdb"))]
    let notes = generate_notes(500);
    #[cfg(any(feature = "sqlite", feature = "lance", feature = "surrealdb"))]
    let query_embedding = random_embedding(EMBEDDING_DIM);

    #[cfg(feature = "sqlite")]
    {
        let dir = TempDir::new().unwrap();
        let store = rt.block_on(async {
            let s = sqlite_bench::create_store(&dir).await;
            for note in &notes {
                s.upsert(note.clone()).await.unwrap();
            }
            s
        });
        let query = query_embedding.clone();
        group.bench_function("sqlite", |b| {
            b.to_async(&rt).iter(|| async {
                let _ = store.search(&query, 10, None).await.unwrap();
            });
        });
    }

    #[cfg(feature = "lance")]
    {
        let dir = TempDir::new().unwrap();
        let store = rt.block_on(async {
            let s = lance_bench::create_store(&dir).await;
            for note in &notes {
                s.upsert(note.clone()).await.unwrap();
            }
            s
        });
        let query = query_embedding.clone();
        group.bench_function("lance", |b| {
            b.to_async(&rt).iter(|| async {
                let _ = store.search(&query, 10, None).await.unwrap();
            });
        });
    }

    #[cfg(feature = "surrealdb")]
    {
        let dir = TempDir::new().unwrap();
        let store = rt.block_on(async {
            let s = surreal_bench::create_store(&dir).await;
            for note in &notes {
                s.upsert(note.clone()).await.unwrap();
            }
            s
        });
        let query = query_embedding.clone();
        group.bench_function("surrealdb", |b| {
            b.to_async(&rt).iter(|| async {
                let _ = store.search(&query, 10, None).await.unwrap();
            });
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_upsert,
    bench_get,
    bench_list,
    bench_vector_search
);
criterion_main!(benches);
