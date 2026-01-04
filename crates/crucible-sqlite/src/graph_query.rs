//! Graph query executor for SQLite backend
//!
//! Uses crucible-query pipeline for parsing queries into GraphIR, then generates
//! SQL specifically for Crucible's EAV schema.
//!
//! ## Supported Syntaxes
//!
//! - **jaq-style**: `outlinks("Title")`, `inlinks("Title")`, `find("Title")`
//! - **SQL sugar**: `SELECT outlinks FROM 'Title'`
//! - **PGQ MATCH**: `MATCH (a {title: 'X'})-[:wikilink]->(b)`
//!
//! ## Schema
//!
//! This executor works with Crucible's EAV schema:
//! - `entities`: `id`, `type`, `data` (JSON with title, path, etc.)
//! - `relations`: `from_entity_id`, `to_entity_id`, `relation_type`

use async_trait::async_trait;
use crucible_core::traits::graph_query::{GraphQueryError, GraphQueryExecutor, GraphQueryResult};
use crucible_query::{
    ir::{EdgeDirection, GraphIR, PatternElement, QuerySource},
    syntax::{JaqSyntax, PgqSyntax, QuerySyntaxRegistry, QuerySyntaxRegistryBuilder, SqlSugarSyntax},
    transform::{QueryTransform, ValidateTransform},
};
use rusqlite::Connection;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Pipeline-based SQLite graph query executor
///
/// Parses queries using crucible-query's syntax parsers, then generates
/// SQL optimized for Crucible's EAV schema.
pub struct SqliteGraphQueryExecutor {
    conn: Arc<Mutex<Connection>>,
    syntax_registry: QuerySyntaxRegistry,
    validator: ValidateTransform,
}

impl SqliteGraphQueryExecutor {
    /// Create a new executor with a database connection
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        let syntax_registry = QuerySyntaxRegistryBuilder::new()
            .with_syntax(PgqSyntax)      // Priority 50 - MATCH syntax
            .with_syntax(SqlSugarSyntax) // Priority 40 - SELECT outlinks FROM
            .with_syntax(JaqSyntax)      // Priority 30 - outlinks("title")
            .build();

        Self {
            conn,
            syntax_registry,
            validator: ValidateTransform,
        }
    }

    /// Parse a query string into GraphIR using the syntax registry
    fn parse(&self, query: &str) -> Result<GraphIR, GraphQueryError> {
        let ir = self.syntax_registry.parse(query).map_err(|e| {
            GraphQueryError::with_query(format!("Parse error: {}", e), query)
        })?;

        // Apply validation transform
        let ir = self.validator.transform(ir).map_err(|e| {
            GraphQueryError::with_query(format!("Validation error: {}", e), query)
        })?;

        Ok(ir)
    }

    /// Generate SQL for Crucible's EAV schema from GraphIR
    fn generate_sql(&self, ir: &GraphIR) -> Result<(String, Vec<(String, String)>), GraphQueryError> {
        // Determine query type and target
        let title = match &ir.source {
            QuerySource::ByTitle(t) => t.clone(),
            QuerySource::ByPath(p) => p.clone(),
            QuerySource::ById(id) => id.clone(),
            QuerySource::All => {
                // Return all entities
                return Ok((
                    "SELECT id, type, data FROM entities WHERE type = 'note'".to_string(),
                    vec![],
                ));
            }
        };

        // Check if this is a simple find or a traversal
        if ir.pattern.elements.is_empty() {
            // Simple find by title
            return Ok((
                r#"SELECT id, type, data FROM entities
                   WHERE type = 'note'
                   AND json_extract(data, '$.title') = ?1"#.to_string(),
                vec![("1".to_string(), title)],
            ));
        }

        // Determine traversal direction from pattern
        let direction = ir.pattern.elements.iter().find_map(|e| {
            if let PatternElement::Edge(edge) = e {
                Some(edge.direction)
            } else {
                None
            }
        }).unwrap_or(EdgeDirection::Out);

        // Get edge type from pattern
        let edge_type = ir.pattern.elements.iter().find_map(|e| {
            if let PatternElement::Edge(edge) = e {
                edge.edge_type.clone()
            } else {
                None
            }
        }).unwrap_or_else(|| "wikilink".to_string());

        // Generate SQL based on direction
        let sql = match direction {
            EdgeDirection::Out => {
                // outlinks: find entities that the source links TO
                r#"
                    SELECT e2.id, e2.type, e2.data
                    FROM entities e1
                    JOIN relations r ON r.from_entity_id = e1.id
                    JOIN entities e2 ON e2.id = r.to_entity_id
                    WHERE e1.type = 'note'
                    AND json_extract(e1.data, '$.title') = ?1
                    AND r.relation_type = ?2
                "#.to_string()
            }
            EdgeDirection::In => {
                // inlinks: find entities that link TO the source
                r#"
                    SELECT e2.id, e2.type, e2.data
                    FROM entities e1
                    JOIN relations r ON r.to_entity_id = e1.id
                    JOIN entities e2 ON e2.id = r.from_entity_id
                    WHERE e1.type = 'note'
                    AND json_extract(e1.data, '$.title') = ?1
                    AND r.relation_type = ?2
                "#.to_string()
            }
            EdgeDirection::Both | EdgeDirection::Undirected => {
                // neighbors: both directions
                r#"
                    SELECT DISTINCT e2.id, e2.type, e2.data
                    FROM entities e1
                    JOIN relations r ON r.from_entity_id = e1.id OR r.to_entity_id = e1.id
                    JOIN entities e2 ON (
                        (e2.id = r.to_entity_id AND r.from_entity_id = e1.id) OR
                        (e2.id = r.from_entity_id AND r.to_entity_id = e1.id)
                    )
                    WHERE e1.type = 'note'
                    AND json_extract(e1.data, '$.title') = ?1
                    AND r.relation_type = ?2
                    AND e2.id != e1.id
                "#.to_string()
            }
        };

        Ok((sql, vec![
            ("1".to_string(), title),
            ("2".to_string(), edge_type),
        ]))
    }

    /// Execute SQL and return JSON results
    fn execute_sql(
        conn: &Connection,
        sql: &str,
        params: &[(String, String)],
    ) -> Result<Vec<Value>, GraphQueryError> {
        let mut stmt = conn.prepare(sql).map_err(|e| {
            GraphQueryError::new(format!("Failed to prepare statement: {} (SQL: {})", e, sql))
        })?;

        // Build positional parameters
        let param_values: Vec<&str> = params.iter().map(|(_, v)| v.as_str()).collect();

        let rows = stmt
            .query_map(rusqlite::params_from_iter(param_values.iter()), |row| {
                let id: String = row.get(0)?;
                let entity_type: String = row.get(1)?;
                let data: String = row.get(2)?;

                // Parse the JSON data
                let mut result: serde_json::Map<String, Value> = serde_json::from_str(&data)
                    .unwrap_or_else(|_| serde_json::Map::new());

                // Add id and type to result
                result.insert("id".to_string(), Value::String(id));
                result.insert("type".to_string(), Value::String(entity_type));

                Ok(Value::Object(result))
            })
            .map_err(|e| GraphQueryError::new(format!("Query execution failed: {}", e)))?;

        let results: Vec<Value> = rows.filter_map(|r| r.ok()).collect();
        Ok(results)
    }
}

#[async_trait]
impl GraphQueryExecutor for SqliteGraphQueryExecutor {
    async fn execute(&self, query: &str) -> GraphQueryResult<Vec<Value>> {
        // Phase 1: Parse query to GraphIR using syntax registry
        let ir = self.parse(query)?;

        // Phase 2: Generate SQL for Crucible's schema
        let (sql, params) = self.generate_sql(&ir)?;

        // Phase 3: Execute against database
        let conn = self.conn.lock().await;
        Self::execute_sql(&conn, &sql, &params)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::apply_migrations;

    async fn setup_pipeline_executor() -> SqliteGraphQueryExecutor {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        apply_migrations(&conn).unwrap();

        // Insert test entities
        conn.execute_batch(
            r#"
            INSERT INTO entities (id, type, data) VALUES
                ('note:index', 'note', '{"title": "Index", "path": "index.md"}'),
                ('note:a', 'note', '{"title": "Note A", "path": "a.md"}'),
                ('note:b', 'note', '{"title": "Note B", "path": "b.md"}'),
                ('note:c', 'note', '{"title": "Note C", "path": "c.md"}');

            INSERT INTO relations (id, from_entity_id, to_entity_id, relation_type) VALUES
                ('rel:1', 'note:index', 'note:a', 'wikilink'),
                ('rel:2', 'note:index', 'note:b', 'wikilink'),
                ('rel:3', 'note:a', 'note:b', 'wikilink'),
                ('rel:4', 'note:c', 'note:index', 'wikilink');
            "#,
        )
        .unwrap();

        SqliteGraphQueryExecutor::new(Arc::new(Mutex::new(conn)))
    }

    // =========================================================================
    // Pipeline parsing tests
    // =========================================================================

    #[test]
    fn test_syntax_registry_has_all_syntaxes() {
        let executor = SqliteGraphQueryExecutor::new(
            Arc::new(Mutex::new(Connection::open_in_memory().unwrap()))
        );
        let names = executor.syntax_registry.syntax_names();

        assert!(names.contains(&"sql-pgq"));
        assert!(names.contains(&"sql-sugar"));
        assert!(names.contains(&"jaq"));
    }

    #[test]
    fn test_parse_jaq_syntax() {
        let executor = SqliteGraphQueryExecutor::new(
            Arc::new(Mutex::new(Connection::open_in_memory().unwrap()))
        );

        let ir = executor.parse(r#"outlinks("Index")"#).unwrap();
        assert!(matches!(ir.source, QuerySource::ByTitle(ref t) if t == "Index"));
    }

    #[test]
    fn test_parse_sql_sugar_syntax() {
        let executor = SqliteGraphQueryExecutor::new(
            Arc::new(Mutex::new(Connection::open_in_memory().unwrap()))
        );

        let ir = executor.parse("SELECT outlinks FROM 'Index'").unwrap();
        assert!(matches!(ir.source, QuerySource::ByTitle(ref t) if t == "Index"));
    }

    #[test]
    fn test_parse_pgq_match_syntax() {
        let executor = SqliteGraphQueryExecutor::new(
            Arc::new(Mutex::new(Connection::open_in_memory().unwrap()))
        );

        let ir = executor.parse("MATCH (a {title: 'Index'})-[:wikilink]->(b)").unwrap();
        assert!(matches!(ir.source, QuerySource::ByTitle(ref t) if t == "Index"));
    }

    // =========================================================================
    // jaq syntax execution tests
    // =========================================================================

    #[tokio::test]
    async fn test_jaq_outlinks() {
        let executor = setup_pipeline_executor().await;

        let results = executor.execute(r#"outlinks("Index")"#).await.unwrap();

        // Index links to Note A and Note B
        assert_eq!(results.len(), 2, "Index should have 2 outlinks");

        let titles: Vec<&str> = results
            .iter()
            .filter_map(|v| v.get("title").and_then(|t| t.as_str()))
            .collect();

        assert!(titles.contains(&"Note A"), "Should include Note A");
        assert!(titles.contains(&"Note B"), "Should include Note B");
    }

    #[tokio::test]
    async fn test_jaq_inlinks() {
        let executor = setup_pipeline_executor().await;

        let results = executor.execute(r#"inlinks("Index")"#).await.unwrap();

        // Note C links to Index
        assert_eq!(results.len(), 1, "Index should have 1 inlink");

        let title = results[0].get("title").and_then(|t| t.as_str());
        assert_eq!(title, Some("Note C"), "Inlink should be Note C");
    }

    #[tokio::test]
    async fn test_jaq_find() {
        let executor = setup_pipeline_executor().await;

        let results = executor.execute(r#"find("Index")"#).await.unwrap();

        assert_eq!(results.len(), 1, "Should find exactly one note");
        let title = results[0].get("title").and_then(|t| t.as_str());
        assert_eq!(title, Some("Index"));
    }

    #[tokio::test]
    async fn test_jaq_neighbors() {
        let executor = setup_pipeline_executor().await;

        let results = executor.execute(r#"neighbors("Index")"#).await.unwrap();

        // Index has outlinks to A, B and inlink from C
        assert_eq!(results.len(), 3, "Index should have 3 neighbors");
    }

    // =========================================================================
    // SQL sugar syntax execution tests
    // =========================================================================

    #[tokio::test]
    async fn test_sql_sugar_outlinks() {
        let executor = setup_pipeline_executor().await;

        let results = executor
            .execute("SELECT outlinks FROM 'Index'")
            .await
            .unwrap();

        assert_eq!(results.len(), 2, "SQL sugar outlinks should return 2 results");
    }

    #[tokio::test]
    async fn test_sql_sugar_inlinks() {
        let executor = setup_pipeline_executor().await;

        let results = executor
            .execute("SELECT inlinks FROM 'Index'")
            .await
            .unwrap();

        assert_eq!(results.len(), 1, "SQL sugar inlinks should return 1 result");
    }

    // =========================================================================
    // PGQ MATCH syntax execution tests
    // =========================================================================

    #[tokio::test]
    async fn test_pgq_match_outlinks() {
        let executor = setup_pipeline_executor().await;

        let results = executor
            .execute("MATCH (a {title: 'Index'})-[:wikilink]->(b)")
            .await
            .unwrap();

        assert_eq!(results.len(), 2, "PGQ MATCH outlinks should return 2 results");
    }

    #[tokio::test]
    async fn test_pgq_match_inlinks() {
        let executor = setup_pipeline_executor().await;

        let results = executor
            .execute("MATCH (a {title: 'Index'})<-[:wikilink]-(b)")
            .await
            .unwrap();

        // Find notes that link TO Index (inlinks)
        assert_eq!(results.len(), 1, "PGQ MATCH inlinks should return 1 result");
    }

    // =========================================================================
    // Error handling tests
    // =========================================================================

    #[tokio::test]
    async fn test_invalid_syntax_error() {
        let executor = setup_pipeline_executor().await;

        let result = executor.execute("INVALID QUERY").await;

        assert!(result.is_err(), "Invalid syntax should return error");
        let err = result.unwrap_err();
        assert!(
            err.message.contains("Parse error"),
            "Error should mention parse: {}",
            err.message
        );
    }

    #[tokio::test]
    async fn test_unknown_function_error() {
        let executor = setup_pipeline_executor().await;

        let result = executor.execute(r#"unknown("arg")"#).await;

        assert!(result.is_err(), "Unknown function should return error");
    }

    // =========================================================================
    // Edge cases
    // =========================================================================

    #[tokio::test]
    async fn test_nonexistent_note() {
        let executor = setup_pipeline_executor().await;

        let results = executor.execute(r#"outlinks("Nonexistent")"#).await.unwrap();

        assert!(results.is_empty(), "Nonexistent note should return empty results");
    }

    #[tokio::test]
    async fn test_note_with_no_links() {
        let executor = setup_pipeline_executor().await;

        // Note B has no outlinks
        let results = executor.execute(r#"outlinks("Note B")"#).await.unwrap();

        assert!(results.is_empty(), "Note B should have no outlinks");
    }

    #[tokio::test]
    async fn test_single_quotes_in_jaq() {
        let executor = setup_pipeline_executor().await;

        let results = executor.execute(r#"outlinks('Index')"#).await.unwrap();

        assert_eq!(results.len(), 2, "Single quotes should work");
    }

    // =========================================================================
    // Result format tests
    // =========================================================================

    #[tokio::test]
    async fn test_result_contains_title() {
        let executor = setup_pipeline_executor().await;

        let results = executor.execute(r#"find("Index")"#).await.unwrap();

        assert!(!results.is_empty());
        assert!(
            results[0].get("title").is_some(),
            "Result should contain title field"
        );
    }

    #[tokio::test]
    async fn test_result_contains_path() {
        let executor = setup_pipeline_executor().await;

        let results = executor.execute(r#"find("Index")"#).await.unwrap();

        assert!(!results.is_empty());
        assert!(
            results[0].get("path").is_some(),
            "Result should contain path field"
        );
        assert_eq!(
            results[0].get("path").and_then(|p| p.as_str()),
            Some("index.md")
        );
    }

    #[tokio::test]
    async fn test_result_contains_id() {
        let executor = setup_pipeline_executor().await;

        let results = executor.execute(r#"find("Index")"#).await.unwrap();

        assert!(!results.is_empty());
        assert!(
            results[0].get("id").is_some(),
            "Result should contain id field"
        );
        assert_eq!(
            results[0].get("id").and_then(|id| id.as_str()),
            Some("note:index")
        );
    }
}
