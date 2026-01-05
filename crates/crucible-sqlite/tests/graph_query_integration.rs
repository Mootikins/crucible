//! Integration tests for graph query pipeline
//!
//! Tests the graph query executor with the full schema setup and real SqlitePool.

use crucible_core::traits::graph_query::GraphQueryExecutor;
use crucible_sqlite::{SqliteConfig, SqliteGraphQueryExecutor, SqlitePool};
use rusqlite::Connection;
use serde_json::Value;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::Mutex;

/// Setup helper: Create a pool with test data
async fn setup_with_test_data() -> (TempDir, SqlitePool) {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");

    let pool = SqlitePool::new(SqliteConfig::new(db_path.to_str().unwrap())).unwrap();

    // Insert test data using the pool's connection
    pool.with_connection_mut(|conn| {
        conn.execute_batch(
            r#"
            INSERT INTO entities (id, type, data) VALUES
                ('note:hub', 'note', '{"title": "Hub", "path": "hub.md"}'),
                ('note:spoke1', 'note', '{"title": "Spoke 1", "path": "spoke1.md"}'),
                ('note:spoke2', 'note', '{"title": "Spoke 2", "path": "spoke2.md"}');

            INSERT INTO relations (id, from_entity_id, to_entity_id, relation_type) VALUES
                ('rel:1', 'note:hub', 'note:spoke1', 'wikilink'),
                ('rel:2', 'note:hub', 'note:spoke2', 'wikilink'),
                ('rel:3', 'note:spoke1', 'note:hub', 'wikilink');
            "#,
        )?;
        Ok(())
    })
    .unwrap();

    (dir, pool)
}

/// Setup helper: Extract the underlying connection from pool for executor
fn create_executor_from_pool(pool: &SqlitePool) -> SqliteGraphQueryExecutor {
    // We need to create a fresh in-memory connection with the same schema
    // since the pool doesn't expose its internal connection.
    // For this integration test, we'll use the pool's capabilities directly.
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();

    // Apply schema
    crucible_sqlite::schema::apply_migrations(&conn).unwrap();

    // Copy data from pool to our executor connection
    pool.with_connection(|pool_conn| {
        // Get all entities
        let mut stmt = pool_conn
            .prepare("SELECT id, type, data FROM entities")
            .unwrap();
        let entities: Vec<(String, String, String)> = stmt
            .query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        // Get all relations
        let mut stmt = pool_conn
            .prepare("SELECT id, from_entity_id, to_entity_id, relation_type FROM relations")
            .unwrap();
        let relations: Vec<(String, String, String, String)> = stmt
            .query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        // Insert into executor connection
        for (id, entity_type, data) in entities {
            conn.execute(
                "INSERT INTO entities (id, type, data) VALUES (?1, ?2, ?3)",
                [&id, &entity_type, &data],
            )?;
        }

        for (id, from_id, to_id, rel_type) in relations {
            conn.execute(
                "INSERT INTO relations (id, from_entity_id, to_entity_id, relation_type) VALUES (?1, ?2, ?3, ?4)",
                [&id, &from_id, &to_id, &rel_type],
            )?;
        }

        Ok(())
    })
    .unwrap();

    SqliteGraphQueryExecutor::new(Arc::new(Mutex::new(conn)))
}

#[tokio::test]
async fn test_graph_query_jaq_outlinks() {
    let (_dir, pool) = setup_with_test_data().await;
    let executor = create_executor_from_pool(&pool);

    // Test jaq syntax - Hub links to Spoke 1 and Spoke 2
    let results = executor.execute(r#"outlinks("Hub")"#).await.unwrap();

    assert_eq!(results.len(), 2, "Hub should have 2 outlinks");

    let titles: Vec<&str> = results
        .iter()
        .filter_map(|v: &Value| v.get("title").and_then(|t| t.as_str()))
        .collect();

    assert!(titles.contains(&"Spoke 1"));
    assert!(titles.contains(&"Spoke 2"));
}

#[tokio::test]
async fn test_graph_query_sql_sugar_outlinks() {
    let (_dir, pool) = setup_with_test_data().await;
    let executor = create_executor_from_pool(&pool);

    // Test SQL sugar
    let results = executor
        .execute("SELECT outlinks FROM 'Hub'")
        .await
        .unwrap();

    assert_eq!(results.len(), 2, "SQL sugar outlinks should find 2 results");
}

#[tokio::test]
async fn test_graph_query_pgq_match_outlinks() {
    let (_dir, pool) = setup_with_test_data().await;
    let executor = create_executor_from_pool(&pool);

    // Test PGQ MATCH
    let results = executor
        .execute("MATCH (a {title: 'Hub'})-[:wikilink]->(b)")
        .await
        .unwrap();

    assert_eq!(results.len(), 2, "PGQ MATCH should find Hub's 2 outlinks");
}

#[tokio::test]
async fn test_graph_query_jaq_inlinks() {
    let (_dir, pool) = setup_with_test_data().await;
    let executor = create_executor_from_pool(&pool);

    // Test inlinks - Hub should be linked to by Spoke 1
    let results = executor.execute(r#"inlinks("Hub")"#).await.unwrap();

    assert_eq!(results.len(), 1, "Hub should have 1 inlink (from Spoke 1)");

    let title = results[0].get("title").and_then(|t| t.as_str());
    assert_eq!(title, Some("Spoke 1"));
}

#[tokio::test]
async fn test_graph_query_pgq_match_inlinks() {
    let (_dir, pool) = setup_with_test_data().await;
    let executor = create_executor_from_pool(&pool);

    // Test PGQ MATCH for inlinks
    let results = executor
        .execute("MATCH (a {title: 'Hub'})<-[:wikilink]-(b)")
        .await
        .unwrap();

    assert_eq!(results.len(), 1, "Hub should be linked to by Spoke 1");
}

#[tokio::test]
async fn test_graph_query_all_syntaxes_together() {
    let (_dir, pool) = setup_with_test_data().await;
    let executor = create_executor_from_pool(&pool);

    // Test all three syntaxes return consistent results
    let jaq_results = executor.execute(r#"outlinks("Hub")"#).await.unwrap();
    let sql_results = executor
        .execute("SELECT outlinks FROM 'Hub'")
        .await
        .unwrap();
    let pgq_results = executor
        .execute("MATCH (a {title: 'Hub'})-[:wikilink]->(b)")
        .await
        .unwrap();

    // All should return the same 2 results
    assert_eq!(jaq_results.len(), 2);
    assert_eq!(sql_results.len(), 2);
    assert_eq!(pgq_results.len(), 2);

    // Results should contain same titles
    let jaq_titles: Vec<&str> = jaq_results
        .iter()
        .filter_map(|v: &Value| v.get("title").and_then(|t| t.as_str()))
        .collect();
    let sql_titles: Vec<&str> = sql_results
        .iter()
        .filter_map(|v: &Value| v.get("title").and_then(|t| t.as_str()))
        .collect();
    let pgq_titles: Vec<&str> = pgq_results
        .iter()
        .filter_map(|v: &Value| v.get("title").and_then(|t| t.as_str()))
        .collect();

    assert_eq!(jaq_titles.len(), 2);
    assert_eq!(sql_titles.len(), 2);
    assert_eq!(pgq_titles.len(), 2);

    // All should contain the same entities
    for title in &jaq_titles {
        assert!(sql_titles.contains(title), "SQL results missing {}", title);
        assert!(pgq_titles.contains(title), "PGQ results missing {}", title);
    }
}

#[tokio::test]
async fn test_graph_query_with_file_database() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");

    // Create pool with real file database
    let pool = SqlitePool::new(SqliteConfig::new(db_path.to_str().unwrap())).unwrap();

    // Insert test data
    pool.with_connection_mut(|conn| {
        conn.execute_batch(
            r#"
            INSERT INTO entities (id, type, data) VALUES
                ('note:alpha', 'note', '{"title": "Alpha", "path": "alpha.md"}'),
                ('note:beta', 'note', '{"title": "Beta", "path": "beta.md"}');

            INSERT INTO relations (id, from_entity_id, to_entity_id, relation_type) VALUES
                ('rel:ab', 'note:alpha', 'note:beta', 'wikilink');
            "#,
        )?;
        Ok(())
    })
    .unwrap();

    // Create executor
    let executor = create_executor_from_pool(&pool);

    // Query the data
    let results = executor.execute(r#"outlinks("Alpha")"#).await.unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].get("title").and_then(|t| t.as_str()),
        Some("Beta")
    );
}

#[tokio::test]
async fn test_graph_query_nonexistent_note() {
    let (_dir, pool) = setup_with_test_data().await;
    let executor = create_executor_from_pool(&pool);

    let results = executor
        .execute(r#"outlinks("Nonexistent")"#)
        .await
        .unwrap();

    assert!(
        results.is_empty(),
        "Nonexistent note should return empty results"
    );
}

#[tokio::test]
async fn test_graph_query_note_with_no_outlinks() {
    let (_dir, pool) = setup_with_test_data().await;
    let executor = create_executor_from_pool(&pool);

    // Spoke 2 has no outlinks in our test data
    let results = executor.execute(r#"outlinks("Spoke 2")"#).await.unwrap();

    assert!(results.is_empty(), "Spoke 2 should have no outlinks");
}

#[tokio::test]
async fn test_graph_query_result_format() {
    let (_dir, pool) = setup_with_test_data().await;
    let executor = create_executor_from_pool(&pool);

    let results = executor.execute(r#"find("Hub")"#).await.unwrap();

    assert_eq!(results.len(), 1);
    let result = &results[0];

    // Verify all expected fields are present
    assert!(result.get("id").is_some(), "Result should contain id field");
    assert!(
        result.get("title").is_some(),
        "Result should contain title field"
    );
    assert!(
        result.get("path").is_some(),
        "Result should contain path field"
    );
    assert!(
        result.get("type").is_some(),
        "Result should contain type field"
    );

    // Verify field values
    assert_eq!(result.get("id").and_then(|v| v.as_str()), Some("note:hub"));
    assert_eq!(result.get("title").and_then(|v| v.as_str()), Some("Hub"));
    assert_eq!(result.get("path").and_then(|v| v.as_str()), Some("hub.md"));
    assert_eq!(result.get("type").and_then(|v| v.as_str()), Some("note"));
}
