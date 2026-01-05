//! Resource Usage Comparison Benchmarks
//!
//! Compares memory usage, disk size, and startup time across SQLite and SurrealDB backends.
//!
//! Run with:
//! ```bash
//! cargo bench -p crucible-benchmarks --features sqlite,surrealdb -- resource
//! ```

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use crucible_benchmarks::fixtures::{generate_graph, seeds, sizes};
use crucible_core::storage::NoteStore;
use std::fs;
use std::time::Instant;
use tempfile::TempDir;

// =============================================================================
// Memory Measurement Utilities
// =============================================================================

/// Get current process RSS (Resident Set Size) in bytes
/// Returns None on non-Linux platforms
#[cfg(target_os = "linux")]
fn get_rss_bytes() -> Option<usize> {
    let statm = fs::read_to_string("/proc/self/statm").ok()?;
    let fields: Vec<&str> = statm.split_whitespace().collect();
    // Field 1 is RSS in pages
    let rss_pages: usize = fields.get(1)?.parse().ok()?;
    // Page size is typically 4096 bytes
    let page_size = 4096;
    Some(rss_pages * page_size)
}

#[cfg(not(target_os = "linux"))]
fn get_rss_bytes() -> Option<usize> {
    None // Memory measurement not supported on this platform
}

fn format_bytes(bytes: usize) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.2} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.2} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}

// =============================================================================
// SQLite Backend
// =============================================================================

#[cfg(feature = "sqlite")]
mod sqlite_bench {
    use super::*;
    use crucible_sqlite::{SqliteConfig, SqlitePool};
    use std::path::Path;

    pub struct SqliteMetrics {
        pub db_size_bytes: u64,
        pub rss_before: Option<usize>,
        pub rss_after: Option<usize>,
        pub setup_duration_ms: u128,
    }

    pub async fn measure_resources(
        dir: &TempDir,
        note_count: usize,
        avg_links: usize,
    ) -> SqliteMetrics {
        let rss_before = get_rss_bytes();
        let start = Instant::now();

        let db_path = dir.path().join("bench.db");
        let config = SqliteConfig::new(db_path.to_string_lossy().as_ref());
        let pool = SqlitePool::new(config).expect("Failed to create SQLite pool");

        let note_store = crucible_sqlite::create_note_store(pool.clone())
            .await
            .expect("Failed to create SQLite store");

        // Generate and insert graph
        let fixture = generate_graph(note_count, avg_links, 0.05, seeds::DEFAULT);

        for note in &fixture.notes {
            note_store.upsert(note.clone()).await.unwrap();
        }

        let setup_duration_ms = start.elapsed().as_millis();
        let rss_after = get_rss_bytes();

        // Measure disk size
        let db_size_bytes = fs::metadata(&db_path).map(|m| m.len()).unwrap_or(0);

        SqliteMetrics {
            db_size_bytes,
            rss_before,
            rss_after,
            setup_duration_ms,
        }
    }

    pub async fn cold_startup(db_path: &Path) -> std::time::Duration {
        let start = Instant::now();
        let config = SqliteConfig::new(db_path.to_string_lossy().as_ref());
        let _pool = SqlitePool::new(config).expect("Failed to create SQLite pool");
        start.elapsed()
    }
}

// =============================================================================
// SurrealDB Backend
// =============================================================================

#[cfg(feature = "surrealdb")]
mod surreal_bench {
    use super::*;
    use crucible_surrealdb::{adapters, SurrealDbConfig};
    use std::path::Path;

    pub struct SurrealMetrics {
        pub db_size_bytes: u64,
        pub rss_before: Option<usize>,
        pub rss_after: Option<usize>,
        pub setup_duration_ms: u128,
    }

    pub async fn measure_resources(
        dir: &TempDir,
        note_count: usize,
        avg_links: usize,
    ) -> SurrealMetrics {
        let rss_before = get_rss_bytes();
        let start = Instant::now();

        let db_path = dir.path().join("surreal.db");
        let config = SurrealDbConfig {
            path: format!("rocksdb:{}", db_path.display()),
            namespace: "bench".to_string(),
            database: "notes".to_string(),
            max_connections: None,
            timeout_seconds: None,
        };

        let handle = adapters::create_surreal_client(config)
            .await
            .expect("Failed to create SurrealDB client");

        let note_store = handle.as_note_store();

        // Generate and insert graph
        let fixture = generate_graph(note_count, avg_links, 0.05, seeds::DEFAULT);

        for note in &fixture.notes {
            note_store.upsert(note.clone()).await.unwrap();
        }

        let setup_duration_ms = start.elapsed().as_millis();
        let rss_after = get_rss_bytes();

        // Measure disk size (SurrealDB creates a directory)
        let db_size_bytes = dir_size(&db_path);

        SurrealMetrics {
            db_size_bytes,
            rss_before,
            rss_after,
            setup_duration_ms,
        }
    }

    pub async fn cold_startup(db_path: &Path) -> std::time::Duration {
        let start = Instant::now();
        let config = SurrealDbConfig {
            path: format!("rocksdb:{}", db_path.display()),
            namespace: "bench".to_string(),
            database: "notes".to_string(),
            max_connections: None,
            timeout_seconds: None,
        };
        let _handle = adapters::create_surreal_client(config)
            .await
            .expect("Failed to create SurrealDB client");
        start.elapsed()
    }

    /// Calculate total size of a directory recursively
    fn dir_size(path: &Path) -> u64 {
        if path.is_file() {
            return fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        }

        let mut total = 0u64;
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    total += fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                } else if path.is_dir() {
                    total += dir_size(&path);
                }
            }
        }
        total
    }
}

// =============================================================================
// Resource Measurement (not Criterion - just prints results)
// =============================================================================

fn bench_resources(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    // We'll use Criterion's custom measurement for disk size
    let mut group = c.benchmark_group("resource/disk_size");
    group.sample_size(10); // Fewer samples for resource tests

    for (label, (note_count, avg_links)) in [
        ("power_user", sizes::POWER_USER),
        ("small_team", sizes::SMALL_TEAM),
    ] {
        #[cfg(feature = "sqlite")]
        {
            let dir = TempDir::new().unwrap();
            let metrics = rt.block_on(sqlite_bench::measure_resources(&dir, note_count, avg_links));

            println!("\n=== SQLite ({}) ===", label);
            println!(
                "  Disk size: {}",
                format_bytes(metrics.db_size_bytes as usize)
            );
            println!("  Setup time: {}ms", metrics.setup_duration_ms);
            if let (Some(before), Some(after)) = (metrics.rss_before, metrics.rss_after) {
                println!(
                    "  RSS delta: {} -> {} ({})",
                    format_bytes(before),
                    format_bytes(after),
                    format_bytes(after.saturating_sub(before))
                );
            }

            // Benchmark cold startup
            group.bench_function(BenchmarkId::new("sqlite", label), |b| {
                b.iter_custom(|iters| {
                    let mut total = std::time::Duration::ZERO;
                    for _ in 0..iters {
                        let dir = TempDir::new().unwrap();
                        let _ = rt.block_on(sqlite_bench::measure_resources(&dir, 100, 3));
                        total +=
                            rt.block_on(sqlite_bench::cold_startup(&dir.path().join("bench.db")));
                    }
                    total
                });
            });
        }

        #[cfg(feature = "surrealdb")]
        {
            let dir = TempDir::new().unwrap();
            let metrics = rt.block_on(surreal_bench::measure_resources(
                &dir, note_count, avg_links,
            ));

            println!("\n=== SurrealDB ({}) ===", label);
            println!(
                "  Disk size: {}",
                format_bytes(metrics.db_size_bytes as usize)
            );
            println!("  Setup time: {}ms", metrics.setup_duration_ms);
            if let (Some(before), Some(after)) = (metrics.rss_before, metrics.rss_after) {
                println!(
                    "  RSS delta: {} -> {} ({})",
                    format_bytes(before),
                    format_bytes(after),
                    format_bytes(after.saturating_sub(before))
                );
            }

            group.bench_function(BenchmarkId::new("surrealdb", label), |b| {
                b.iter_custom(|iters| {
                    let mut total = std::time::Duration::ZERO;
                    for _ in 0..iters {
                        let dir = TempDir::new().unwrap();
                        let _ = rt.block_on(surreal_bench::measure_resources(&dir, 100, 3));
                        total += rt
                            .block_on(surreal_bench::cold_startup(&dir.path().join("surreal.db")));
                    }
                    total
                });
            });
        }
    }

    group.finish();
}

// =============================================================================
// Startup Time Benchmarks
// =============================================================================

fn bench_startup(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("resource/startup");
    group.sample_size(20);

    // Create databases first
    #[cfg(feature = "sqlite")]
    {
        let dir = TempDir::new().unwrap();
        let _ = rt.block_on(sqlite_bench::measure_resources(&dir, 1000, 5));
        let db_path = dir.path().join("bench.db");

        group.bench_function("sqlite/cold", |b| {
            b.to_async(&rt)
                .iter(|| sqlite_bench::cold_startup(&db_path));
        });
    }

    #[cfg(feature = "surrealdb")]
    {
        let dir = TempDir::new().unwrap();
        let _ = rt.block_on(surreal_bench::measure_resources(&dir, 1000, 5));
        let db_path = dir.path().join("surreal.db");

        group.bench_function("surrealdb/cold", |b| {
            b.to_async(&rt)
                .iter(|| surreal_bench::cold_startup(&db_path));
        });
    }

    group.finish();
}

// =============================================================================
// Benchmark Registration
// =============================================================================

criterion_group!(resource_benchmarks, bench_resources, bench_startup);
criterion_main!(resource_benchmarks);
