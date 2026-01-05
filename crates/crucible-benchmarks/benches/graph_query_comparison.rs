//! Graph Query Comparison Benchmarks
//!
//! Compares graph traversal performance across SQLite and SurrealDB backends.
//!
//! Run with:
//! ```bash
//! # All backends
//! cargo bench -p crucible-benchmarks --features sqlite,surrealdb -- graph
//!
//! # Individual backends
//! cargo bench -p crucible-benchmarks --features sqlite -- graph
//! cargo bench -p crucible-benchmarks --features surrealdb -- graph
//! ```

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use crucible_benchmarks::fixtures::{generate_graph, seeds, sizes};
use crucible_core::storage::NoteStore;
use crucible_core::traits::GraphQueryExecutor;
use std::sync::Arc;
use tempfile::TempDir;

// =============================================================================
// SQLite Backend
// =============================================================================

#[cfg(feature = "sqlite")]
mod sqlite_bench {
    use super::*;
    use crucible_sqlite::{SqliteConfig, SqliteGraphQueryExecutor, SqlitePool};
    use rusqlite::Connection;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    pub struct SqliteFixture {
        pub graph_executor: SqliteGraphQueryExecutor,
        pub hub_title: String,
        // Hold onto these to keep them alive during benchmark
        _pool: SqlitePool,
    }

    pub async fn setup(dir: &TempDir, note_count: usize, avg_links: usize) -> SqliteFixture {
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

        // Open a second connection for the graph executor (uses tokio::sync::Mutex)
        // This is needed because SqlitePool uses parking_lot::Mutex
        let conn = Connection::open(&db_path).expect("Failed to open connection for graph executor");
        let graph_executor = SqliteGraphQueryExecutor::new(Arc::new(Mutex::new(conn)));

        SqliteFixture {
            graph_executor,
            hub_title: fixture.stats.hub_notes.first().cloned().unwrap_or_default(),
            _pool: pool,
        }
    }
}

// =============================================================================
// SurrealDB Backend
// =============================================================================

#[cfg(feature = "surrealdb")]
mod surreal_bench {
    use super::*;
    use crucible_core::storage::NoteStore;
    use crucible_core::traits::GraphQueryExecutor;
    use crucible_surrealdb::{adapters, SurrealDbConfig};
    use std::sync::Arc;

    pub struct SurrealFixture {
        pub graph_executor: Arc<dyn GraphQueryExecutor>,
        pub hub_title: String,
        // Hold onto these to keep them alive during benchmark
        _note_store: Arc<dyn NoteStore>,
    }

    pub async fn setup(_dir: &TempDir, note_count: usize, avg_links: usize) -> SurrealFixture {
        // SurrealDB uses in-memory for benchmarks
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

        // Generate and insert graph
        let fixture = generate_graph(note_count, avg_links, 0.05, seeds::DEFAULT);

        for note in &fixture.notes {
            note_store.upsert(note.clone()).await.unwrap();
        }

        // Create graph executor
        let graph_executor = adapters::create_graph_executor(handle);

        SurrealFixture {
            graph_executor,
            hub_title: fixture.stats.hub_notes.first().cloned().unwrap_or_default(),
            _note_store: note_store,
        }
    }
}

// =============================================================================
// Single-Hop Traversal Benchmarks
// =============================================================================

fn bench_outlinks(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("graph/outlinks");

    for (label, (note_count, avg_links)) in [
        ("power_user", sizes::POWER_USER),
        ("small_team", sizes::SMALL_TEAM),
    ] {
        #[cfg(feature = "sqlite")]
        {
            let dir = TempDir::new().unwrap();
            let fixture = rt.block_on(sqlite_bench::setup(&dir, note_count, avg_links));
            let query = format!(r#"outlinks("{}")"#, fixture.hub_title);

            group.bench_with_input(
                BenchmarkId::new("sqlite", label),
                &query,
                |b, query| {
                    b.to_async(&rt)
                        .iter(|| async { fixture.graph_executor.execute(query).await.unwrap() });
                },
            );
        }

        #[cfg(feature = "surrealdb")]
        {
            let dir = TempDir::new().unwrap();
            let fixture = rt.block_on(surreal_bench::setup(&dir, note_count, avg_links));
            let query = format!(r#"outlinks("{}")"#, fixture.hub_title);

            group.bench_with_input(
                BenchmarkId::new("surrealdb", label),
                &query,
                |b, query| {
                    b.to_async(&rt)
                        .iter(|| async { fixture.graph_executor.execute(query).await.unwrap() });
                },
            );
        }
    }

    group.finish();
}

fn bench_inlinks(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("graph/inlinks");

    for (label, (note_count, avg_links)) in [
        ("power_user", sizes::POWER_USER),
        ("small_team", sizes::SMALL_TEAM),
    ] {
        #[cfg(feature = "sqlite")]
        {
            let dir = TempDir::new().unwrap();
            let fixture = rt.block_on(sqlite_bench::setup(&dir, note_count, avg_links));
            let query = format!(r#"inlinks("{}")"#, fixture.hub_title);

            group.bench_with_input(
                BenchmarkId::new("sqlite", label),
                &query,
                |b, query| {
                    b.to_async(&rt)
                        .iter(|| async { fixture.graph_executor.execute(query).await.unwrap() });
                },
            );
        }

        #[cfg(feature = "surrealdb")]
        {
            let dir = TempDir::new().unwrap();
            let fixture = rt.block_on(surreal_bench::setup(&dir, note_count, avg_links));
            let query = format!(r#"inlinks("{}")"#, fixture.hub_title);

            group.bench_with_input(
                BenchmarkId::new("surrealdb", label),
                &query,
                |b, query| {
                    b.to_async(&rt)
                        .iter(|| async { fixture.graph_executor.execute(query).await.unwrap() });
                },
            );
        }
    }

    group.finish();
}

fn bench_neighbors(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("graph/neighbors");

    for (label, (note_count, avg_links)) in [
        ("power_user", sizes::POWER_USER),
        ("small_team", sizes::SMALL_TEAM),
    ] {
        #[cfg(feature = "sqlite")]
        {
            let dir = TempDir::new().unwrap();
            let fixture = rt.block_on(sqlite_bench::setup(&dir, note_count, avg_links));
            let query = format!(r#"neighbors("{}")"#, fixture.hub_title);

            group.bench_with_input(
                BenchmarkId::new("sqlite", label),
                &query,
                |b, query| {
                    b.to_async(&rt)
                        .iter(|| async { fixture.graph_executor.execute(query).await.unwrap() });
                },
            );
        }

        #[cfg(feature = "surrealdb")]
        {
            let dir = TempDir::new().unwrap();
            let fixture = rt.block_on(surreal_bench::setup(&dir, note_count, avg_links));
            let query = format!(r#"neighbors("{}")"#, fixture.hub_title);

            group.bench_with_input(
                BenchmarkId::new("surrealdb", label),
                &query,
                |b, query| {
                    b.to_async(&rt)
                        .iter(|| async { fixture.graph_executor.execute(query).await.unwrap() });
                },
            );
        }
    }

    group.finish();
}

// =============================================================================
// Raw SQL/SurrealQL Benchmarks (bypassing query pipeline)
// =============================================================================

/// Raw SQL benchmarks for SQLite - tests pure database performance
#[cfg(feature = "sqlite")]
mod sqlite_raw {
    use rusqlite::Connection;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    /// Single-hop outlinks via raw SQL
    /// Notes table stores links_to as JSON array
    pub async fn outlinks(conn: &Arc<Mutex<Connection>>, source_path: &str) -> Vec<String> {
        let conn = conn.lock().await;
        let mut stmt = conn
            .prepare_cached(
                r#"SELECT links_to FROM notes WHERE path = ?1"#,
            )
            .unwrap();

        let links_json: Option<String> = stmt
            .query_row([source_path], |row| row.get(0))
            .ok();

        links_json
            .and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok())
            .unwrap_or_default()
    }

    /// Single-hop inlinks via raw SQL using junction table
    /// O(log n) via index on note_links(target_path)
    pub async fn inlinks(conn: &Arc<Mutex<Connection>>, target_path: &str) -> Vec<String> {
        let conn = conn.lock().await;
        let mut stmt = conn
            .prepare_cached(
                r#"SELECT source_path FROM note_links WHERE target_path = ?1"#,
            )
            .unwrap();
        let rows = stmt
            .query_map([target_path], |row| row.get(0))
            .unwrap();
        rows.filter_map(|r| r.ok()).collect()
    }

    /// Single-hop inlinks via LIKE (old method for comparison)
    /// O(n) full table scan
    pub async fn inlinks_like(conn: &Arc<Mutex<Connection>>, target_path: &str) -> Vec<String> {
        let conn = conn.lock().await;
        let mut stmt = conn
            .prepare_cached(
                r#"SELECT path FROM notes WHERE links_to LIKE '%' || ?1 || '%'"#,
            )
            .unwrap();
        let rows = stmt
            .query_map([target_path], |row| row.get(0))
            .unwrap();
        rows.filter_map(|r| r.ok()).collect()
    }

    /// 2-hop outlinks via raw SQL (using json_each for link expansion)
    pub async fn outlinks_2hop(conn: &Arc<Mutex<Connection>>, source_path: &str) -> Vec<String> {
        let conn = conn.lock().await;
        let mut stmt = conn
            .prepare_cached(
                r#"
                WITH hop1 AS (
                    SELECT json_each.value AS path
                    FROM notes, json_each(notes.links_to)
                    WHERE notes.path = ?1
                ),
                hop2 AS (
                    SELECT json_each.value AS path
                    FROM notes, json_each(notes.links_to), hop1
                    WHERE notes.path = hop1.path
                )
                SELECT DISTINCT path FROM hop2
                "#,
            )
            .unwrap();
        let rows = stmt
            .query_map([source_path], |row| row.get(0))
            .unwrap();
        rows.filter_map(|r| r.ok()).collect()
    }

    /// 3-hop outlinks via raw SQL
    pub async fn outlinks_3hop(conn: &Arc<Mutex<Connection>>, source_path: &str) -> Vec<String> {
        let conn = conn.lock().await;
        let mut stmt = conn
            .prepare_cached(
                r#"
                WITH hop1 AS (
                    SELECT json_each.value AS path
                    FROM notes, json_each(notes.links_to)
                    WHERE notes.path = ?1
                ),
                hop2 AS (
                    SELECT json_each.value AS path
                    FROM notes, json_each(notes.links_to), hop1
                    WHERE notes.path = hop1.path
                ),
                hop3 AS (
                    SELECT json_each.value AS path
                    FROM notes, json_each(notes.links_to), hop2
                    WHERE notes.path = hop2.path
                )
                SELECT DISTINCT path FROM hop3
                "#,
            )
            .unwrap();
        let rows = stmt
            .query_map([source_path], |row| row.get(0))
            .unwrap();
        rows.filter_map(|r| r.ok()).collect()
    }
}

/// Raw SurrealQL benchmarks - tests pure database performance
#[cfg(feature = "surrealdb")]
mod surreal_raw {
    use crucible_surrealdb::test_utils::SurrealClient;
    use serde_json::Value;

    /// Single-hop outlinks via raw SurrealQL
    pub async fn outlinks(client: &SurrealClient, source_path: &str) -> Vec<String> {
        let sql = r#"SELECT links_to FROM notes WHERE path = $path"#;
        let result = client
            .query(sql, &[serde_json::json!({"path": source_path})])
            .await
            .unwrap();

        // Extract links_to array from the record's data field
        result
            .records
            .into_iter()
            .flat_map(|r| {
                r.data
                    .get("links_to")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default()
            })
            .collect()
    }

    /// Single-hop inlinks via raw SurrealQL
    pub async fn inlinks(client: &SurrealClient, target_path: &str) -> Vec<String> {
        let sql = r#"SELECT path FROM notes WHERE $path IN links_to"#;
        let result = client
            .query(sql, &[serde_json::json!({"path": target_path})])
            .await
            .unwrap();

        result
            .records
            .into_iter()
            .filter_map(|r| {
                r.data
                    .get("path")
                    .and_then(|v| v.as_str())
                    .map(String::from)
            })
            .collect()
    }

    /// 2-hop outlinks via raw SurrealQL (using subqueries)
    pub async fn outlinks_2hop(client: &SurrealClient, source_path: &str) -> Vec<String> {
        // Get notes 2 hops away by chaining subqueries
        let sql = r#"
            LET $hop1 = (SELECT VALUE links_to FROM notes WHERE path = $path)[0];
            SELECT VALUE array::distinct(array::flatten(
                (SELECT VALUE links_to FROM notes WHERE path IN $hop1)
            ))
        "#;
        let result = client
            .query(sql, &[serde_json::json!({"path": source_path})])
            .await
            .unwrap();

        // VALUE returns raw arrays in the data field
        result
            .records
            .into_iter()
            .flat_map(|r| {
                // When using VALUE, data might be structured differently
                // Try to extract from whatever structure we get
                extract_string_array(&r.data)
            })
            .collect()
    }

    /// 3-hop outlinks via raw SurrealQL
    pub async fn outlinks_3hop(client: &SurrealClient, source_path: &str) -> Vec<String> {
        // 3-hop traversal using LET bindings
        let sql = r#"
            LET $hop1 = (SELECT VALUE links_to FROM notes WHERE path = $path)[0];
            LET $hop2 = array::flatten((SELECT VALUE links_to FROM notes WHERE path IN $hop1));
            SELECT VALUE array::distinct(array::flatten(
                (SELECT VALUE links_to FROM notes WHERE path IN $hop2)
            ))
        "#;
        let result = client
            .query(sql, &[serde_json::json!({"path": source_path})])
            .await
            .unwrap();

        result
            .records
            .into_iter()
            .flat_map(|r| extract_string_array(&r.data))
            .collect()
    }

    /// Helper to extract string array from a HashMap<String, Value>
    fn extract_string_array(data: &std::collections::HashMap<String, Value>) -> Vec<String> {
        // For VALUE queries, results might be in various keys
        for value in data.values() {
            if let Some(arr) = value.as_array() {
                return arr
                    .iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect();
            }
        }
        vec![]
    }
}

fn bench_raw_single_hop(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("graph_raw/single_hop");

    for (label, (note_count, avg_links)) in [
        ("power_user", sizes::POWER_USER),
        ("small_team", sizes::SMALL_TEAM),
    ] {
        #[cfg(feature = "sqlite")]
        {
            let dir = TempDir::new().unwrap();
            let _fixture = rt.block_on(sqlite_bench::setup(&dir, note_count, avg_links));
            let hub_path = "notes/note-00000.md".to_string();

            let conn = Arc::new(tokio::sync::Mutex::new(
                rusqlite::Connection::open(dir.path().join("bench.db")).unwrap(),
            ));

            group.bench_with_input(
                BenchmarkId::new("sqlite/outlinks", label),
                &hub_path,
                |b, path| {
                    b.to_async(&rt)
                        .iter(|| sqlite_raw::outlinks(&conn, path));
                },
            );

            let conn2 = Arc::new(tokio::sync::Mutex::new(
                rusqlite::Connection::open(dir.path().join("bench.db")).unwrap(),
            ));

            group.bench_with_input(
                BenchmarkId::new("sqlite/inlinks", label),
                &hub_path,
                |b, path| {
                    b.to_async(&rt)
                        .iter(|| sqlite_raw::inlinks(&conn2, path));
                },
            );

            // Also benchmark the old LIKE method for comparison
            let conn3 = Arc::new(tokio::sync::Mutex::new(
                rusqlite::Connection::open(dir.path().join("bench.db")).unwrap(),
            ));

            group.bench_with_input(
                BenchmarkId::new("sqlite/inlinks_like", label),
                &hub_path,
                |b, path| {
                    b.to_async(&rt)
                        .iter(|| sqlite_raw::inlinks_like(&conn3, path));
                },
            );
        }

        #[cfg(feature = "surrealdb")]
        {
            use crucible_surrealdb::test_utils::SurrealClient;

            let hub_path = "notes/note-00000.md".to_string();
            let config = crucible_surrealdb::SurrealDbConfig {
                path: ":memory:".to_string(),
                namespace: "bench".to_string(),
                database: "notes".to_string(),
                max_connections: None,
                timeout_seconds: None,
            };

            let client = rt.block_on(SurrealClient::new(config)).unwrap();
            let note_store = crucible_surrealdb::test_utils::SurrealNoteStore::new(client.clone());

            // Insert data
            let graph_fixture = generate_graph(note_count, avg_links, 0.05, seeds::DEFAULT);
            for note in &graph_fixture.notes {
                rt.block_on(note_store.upsert(note.clone())).unwrap();
            }

            group.bench_with_input(
                BenchmarkId::new("surrealdb/outlinks", label),
                &hub_path,
                |b, path| {
                    b.to_async(&rt)
                        .iter(|| surreal_raw::outlinks(&client, path));
                },
            );

            group.bench_with_input(
                BenchmarkId::new("surrealdb/inlinks", label),
                &hub_path,
                |b, path| {
                    b.to_async(&rt)
                        .iter(|| surreal_raw::inlinks(&client, path));
                },
            );
        }
    }

    group.finish();
}

fn bench_raw_multi_hop(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("graph_raw/multi_hop");

    // Use power_user size for multi-hop (10K would be very slow for 3-hop)
    let (note_count, avg_links) = sizes::POWER_USER;
    let hub_path = "notes/note-00000.md".to_string();

    #[cfg(feature = "sqlite")]
    {
        let dir = TempDir::new().unwrap();
        let _fixture = rt.block_on(sqlite_bench::setup(&dir, note_count, avg_links));
        let conn = Arc::new(tokio::sync::Mutex::new(
            rusqlite::Connection::open(dir.path().join("bench.db")).unwrap(),
        ));

        group.bench_with_input(
            BenchmarkId::new("sqlite", "2_hop"),
            &hub_path,
            |b, path| {
                b.to_async(&rt)
                    .iter(|| sqlite_raw::outlinks_2hop(&conn, path));
            },
        );

        group.bench_with_input(
            BenchmarkId::new("sqlite", "3_hop"),
            &hub_path,
            |b, path| {
                b.to_async(&rt)
                    .iter(|| sqlite_raw::outlinks_3hop(&conn, path));
            },
        );
    }

    #[cfg(feature = "surrealdb")]
    {
        use crucible_surrealdb::test_utils::SurrealClient;

        let config = crucible_surrealdb::SurrealDbConfig {
            path: ":memory:".to_string(),
            namespace: "bench".to_string(),
            database: "notes".to_string(),
            max_connections: None,
            timeout_seconds: None,
        };

        let client = rt.block_on(SurrealClient::new(config)).unwrap();
        let note_store = crucible_surrealdb::test_utils::SurrealNoteStore::new(client.clone());

        let graph_fixture = generate_graph(note_count, avg_links, 0.05, seeds::DEFAULT);
        for note in &graph_fixture.notes {
            rt.block_on(note_store.upsert(note.clone())).unwrap();
        }

        group.bench_with_input(
            BenchmarkId::new("surrealdb", "2_hop"),
            &hub_path,
            |b, path| {
                b.to_async(&rt)
                    .iter(|| surreal_raw::outlinks_2hop(&client, path));
            },
        );

        group.bench_with_input(
            BenchmarkId::new("surrealdb", "3_hop"),
            &hub_path,
            |b, path| {
                b.to_async(&rt)
                    .iter(|| surreal_raw::outlinks_3hop(&client, path));
            },
        );
    }

    group.finish();
}

// =============================================================================
// Benchmark Registration
// =============================================================================

criterion_group!(single_hop, bench_outlinks, bench_inlinks, bench_neighbors);
criterion_group!(raw_benchmarks, bench_raw_single_hop, bench_raw_multi_hop);
criterion_main!(single_hop, raw_benchmarks);
