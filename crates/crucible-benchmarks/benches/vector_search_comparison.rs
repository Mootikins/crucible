//! Vector Search Comparison: SQLite brute-force vs LanceDB ANN
//!
//! Tests vector search performance at different dataset sizes to find
//! where LanceDB's approximate nearest neighbor (ANN) indexing outperforms
//! SQLite's brute-force cosine similarity.
//!
//! Run with:
//! ```bash
//! cargo bench -p crucible-storage-tests --features sqlite,lance --bench vector_search_comparison
//! ```

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use crucible_core::parser::BlockHash;
use crucible_core::storage::{NoteRecord, NoteStore};
use rand::Rng;
use std::time::Duration;
use tempfile::TempDir;

// 768 = bge-base, e5-base (common default)
// 384 = bge-small, all-MiniLM-L6
// 1536 = OpenAI ada-002
const EMBEDDING_DIM: usize = 768;

fn random_embedding(dim: usize) -> Vec<f32> {
    let mut rng = rand::rng();
    let v: Vec<f32> = (0..dim).map(|_| rng.random::<f32>()).collect();
    // Normalize for cosine similarity
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        v.into_iter().map(|x| x / norm).collect()
    } else {
        v
    }
}

/// Generate test records (simulating block embeddings)
/// In reality, each note has multiple blocks, each with its own embedding.
/// A kiln with 250 notes averaging 4 blocks = 1000 block embeddings.
fn generate_block_embeddings(count: usize) -> Vec<NoteRecord> {
    (0..count)
        .map(|i| {
            // Simulate block paths: note-X/block-Y
            let note_idx = i / 4; // ~4 blocks per note
            let block_idx = i % 4;
            let path = format!("notes/note-{}/block-{}.md", note_idx, block_idx);
            let hash_bytes: [u8; 32] = rand::rng().random();
            NoteRecord::new(path, BlockHash::new(hash_bytes))
                .with_title(format!("Block {} of Note {}", block_idx, note_idx))
                .with_embedding(random_embedding(EMBEDDING_DIM))
        })
        .collect()
}

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

#[cfg(feature = "lance")]
mod lance_bench {
    use super::*;
    use crucible_lance::LanceNoteStore;

    pub async fn create_store(dir: &TempDir) -> LanceNoteStore {
        let db_path = dir.path().join("bench.lance");
        crucible_lance::create_note_store_with_dimensions(
            db_path.to_string_lossy().as_ref(),
            EMBEDDING_DIM,
        )
        .await
        .expect("Failed to create Lance store")
    }

    pub async fn create_store_with_index(dir: &TempDir, notes: &[NoteRecord]) -> LanceNoteStore {
        let store = create_store(dir).await;
        // Load all notes
        for note in notes {
            store.upsert(note.clone()).await.unwrap();
        }
        // Create index after bulk load (requires 256+ rows for IVF-PQ)
        if notes.len() >= 256 {
            if let Err(e) = store.create_index().await {
                eprintln!("Warning: Failed to create index: {}", e);
            }
        }
        store
    }
}

fn bench_vector_search_scaling(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("vector_search_scaling");

    // Longer measurement time for more stable results
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(50);

    // Test different dataset sizes
    for &count in &[100, 500, 1000, 2000] {
        let notes = generate_block_embeddings(count);
        let query = random_embedding(EMBEDDING_DIM);

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
            let q = query.clone();
            group.bench_with_input(
                BenchmarkId::new("sqlite_bruteforce", count),
                &count,
                |b, _| {
                    b.to_async(&rt)
                        .iter(|| async { store.search(&q, 10, None).await.unwrap() });
                },
            );
        }

        #[cfg(feature = "lance")]
        {
            let dir = TempDir::new().unwrap();
            // Use indexed store for fair comparison
            let store = rt.block_on(lance_bench::create_store_with_index(&dir, &notes));
            let q = query.clone();
            group.bench_with_input(BenchmarkId::new("lance_ann", count), &count, |b, _| {
                b.to_async(&rt)
                    .iter(|| async { store.search(&q, 10, None).await.unwrap() });
            });
        }
    }
    group.finish();
}

fn bench_search_only(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("search_only_1000");

    group.measurement_time(Duration::from_secs(5));

    let notes = generate_block_embeddings(1000);
    let query = random_embedding(EMBEDDING_DIM);

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
        let q = query.clone();
        group.bench_function("sqlite", |b| {
            b.to_async(&rt)
                .iter(|| async { store.search(&q, 10, None).await.unwrap() });
        });
    }

    #[cfg(feature = "lance")]
    {
        let dir = TempDir::new().unwrap();
        // Use indexed store (1000 notes > 256 threshold)
        let store = rt.block_on(lance_bench::create_store_with_index(&dir, &notes));
        let q = query.clone();
        group.bench_function("lance", |b| {
            b.to_async(&rt)
                .iter(|| async { store.search(&q, 10, None).await.unwrap() });
        });
    }

    group.finish();
}

criterion_group!(benches, bench_vector_search_scaling, bench_search_only);
criterion_main!(benches);
