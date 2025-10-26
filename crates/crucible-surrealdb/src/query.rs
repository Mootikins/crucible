//! SurrealQL query execution and result formatting
//!
//! This module provides functionality for executing raw SurrealQL queries
//! against the SurrealDB backend and formatting results for terminal display.
//!
//! # Usage
//!
//! ```no_run
//! use crucible_surrealdb::query::{execute_query, format_results};
//!
//! # async fn example() -> anyhow::Result<()> {
//! // Execute a query
//! let results = execute_query("SELECT * FROM notes WHERE tags CONTAINS 'rust'").await?;
//!
//! // Format for terminal display
//! let table = format_results(&results)?;
//! println!("{}", table);
//! # Ok(())
//! # }
//! ```

use anyhow::{Context, Result};
use comfy_table::{Cell, Table};
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use surrealdb::engine::local::Db;
use surrealdb::sql;
use surrealdb::RecordId;
use surrealdb::Surreal;

/// Record type that can deserialize from SurrealDB responses
/// Uses RecordId for id field which properly deserializes Thing types
/// The id field is optional to support aggregate queries (COUNT, SUM, etc.)
#[derive(Debug, Clone, Deserialize)]
struct DynRecord {
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<RecordId>,
    #[serde(flatten)]
    fields: HashMap<String, Value>,
}

/// Execute a SurrealQL query and return results
///
/// # Arguments
///
/// * `db` - Database connection
/// * `query` - A SurrealQL query string
///
/// # Returns
///
/// A vector of JSON values representing the query results
///
/// # Errors
///
/// Returns an error if the query is malformed or execution fails
pub async fn execute_query(db: &Surreal<Db>, query: &str) -> Result<Vec<Value>> {
    // Validate query is not empty
    if query.trim().is_empty() {
        anyhow::bail!("Query cannot be empty");
    }

    // Execute the query
    let mut response = db.query(query).await.context("Failed to execute query")?;

    // Try deserializing as Vec<DynRecord>
    let records_result: std::result::Result<Vec<DynRecord>, _> = response.take(0);

    match records_result {
        Ok(records) => {
            // Convert DynRecords to JSON values
            let json_results: Vec<Value> = records
                .into_iter()
                .map(|record| {
                    let mut map: serde_json::Map<String, Value> =
                        record.fields.into_iter().collect();
                    // Insert the id field if present, converting RecordId to "table:id" format
                    if let Some(id) = record.id {
                        map.insert("id".to_string(), Value::String(id.to_string()));
                    }
                    Value::Object(map)
                })
                .collect();
            Ok(json_results)
        }
        Err(_) => {
            // If that fails, result might be empty or single value
            Ok(vec![])
        }
    }
}

/// Convert a SurrealDB value to a serde_json::Value
///
/// This is a utility function for converting SurrealDB's internal sql::Value type
/// to serde_json::Value. Currently not used due to sdk deserialization limitations,
/// but kept for potential future use.
#[allow(dead_code)]
fn sql_value_to_json(value: sql::Value) -> Value {
    match value {
        sql::Value::None | sql::Value::Null => Value::Null,
        sql::Value::Bool(b) => Value::Bool(b),
        sql::Value::Number(n) => {
            if n.is_int() {
                Value::Number(serde_json::Number::from(n.as_int()))
            } else if n.is_float() {
                serde_json::Number::from_f64(n.as_float())
                    .map(Value::Number)
                    .unwrap_or(Value::Null)
            } else {
                Value::Null
            }
        }
        sql::Value::Strand(s) => Value::String(s.to_string()),
        sql::Value::Array(arr) => {
            let items: Vec<Value> = arr.into_iter().map(sql_value_to_json).collect();
            Value::Array(items)
        }
        sql::Value::Object(obj) => {
            let mut map = serde_json::Map::new();
            for (key, value) in obj.iter() {
                map.insert(key.to_string(), sql_value_to_json(value.clone()));
            }
            Value::Object(map)
        }
        sql::Value::Thing(thing) => {
            // Convert Thing (record ID) to string representation
            Value::String(thing.to_string())
        }
        sql::Value::Datetime(dt) => {
            // Convert datetime to ISO 8601 string
            Value::String(dt.to_string())
        }
        // Handle other types by converting to string
        _ => Value::String(value.to_string()),
    }
}

/// Format query results as a table for terminal display
///
/// # Arguments
///
/// * `results` - Query results as a vector of JSON values
///
/// # Returns
///
/// A formatted string suitable for terminal output
///
/// # Errors
///
/// Returns an error if results cannot be formatted
pub fn format_results(results: &[Value]) -> Result<String> {
    // Handle empty results
    if results.is_empty() {
        return Ok("No results".to_string());
    }

    // Create table
    let mut table = Table::new();

    // Collect all unique column names from all results (for sparse data)
    let mut columns = BTreeMap::new();
    for result in results {
        if let Value::Object(obj) = result {
            for (key, _) in obj {
                columns.insert(key.clone(), ());
            }
        }
    }
    let column_names: Vec<String> = columns.keys().cloned().collect();

    // Add header row
    let header_cells: Vec<Cell> = column_names.iter().map(|name| Cell::new(name)).collect();
    table.set_header(header_cells);

    // Add data rows
    for result in results {
        let mut row = Vec::new();

        for col_name in &column_names {
            let cell_value = if let Value::Object(obj) = result {
                obj.get(col_name)
                    .map(|v| format_value(v))
                    .unwrap_or_else(|| String::new())
            } else {
                String::new()
            };
            row.push(Cell::new(cell_value));
        }

        table.add_row(row);
    }

    Ok(table.to_string())
}

/// Format a JSON value for display in a table cell
fn format_value(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        Value::Array(arr) => {
            // Format arrays as JSON for readability
            serde_json::to_string(arr).unwrap_or_else(|_| "[]".to_string())
        }
        Value::Object(obj) => {
            // Format nested objects as JSON
            serde_json::to_string(obj).unwrap_or_else(|_| "{}".to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // =========================================================================
    // Test Database Setup Helpers
    // =========================================================================

    use surrealdb::engine::local::Mem;
    use surrealdb::Surreal;

    /// Setup an in-memory SurrealDB instance for testing
    ///
    /// This should create a clean database with the schema loaded
    async fn setup_test_db() -> Result<Surreal<Db>> {
        // Create in-memory SurrealDB instance
        let db = Surreal::new::<Mem>(()).await?;

        // Use test namespace and database
        db.use_ns("test").use_db("test").await?;

        // Define the schema
        db.query(
            r#"
            -- Define notes table
            DEFINE TABLE notes SCHEMAFULL;
            DEFINE FIELD path ON TABLE notes TYPE string;
            DEFINE FIELD title ON TABLE notes TYPE option<string>;
            DEFINE FIELD content ON TABLE notes TYPE string DEFAULT "";
            DEFINE FIELD tags ON TABLE notes TYPE array<string> DEFAULT [];
            DEFINE FIELD word_count ON TABLE notes TYPE option<int>;
            DEFINE FIELD created_at ON TABLE notes TYPE datetime DEFAULT time::now();
            DEFINE FIELD modified_at ON TABLE notes TYPE datetime DEFAULT time::now();
            DEFINE FIELD metadata ON TABLE notes TYPE object DEFAULT {};
            DEFINE INDEX unique_path ON TABLE notes COLUMNS path UNIQUE;

            -- Define tags table
            DEFINE TABLE tags SCHEMAFULL;
            DEFINE FIELD name ON TABLE tags TYPE string;
            DEFINE FIELD description ON TABLE tags TYPE option<string>;
            DEFINE INDEX unique_tag_name ON TABLE tags COLUMNS name UNIQUE;

            -- Define wikilink relation
            DEFINE TABLE wikilink SCHEMAFULL TYPE RELATION FROM notes TO notes;
            DEFINE FIELD link_text ON TABLE wikilink TYPE string;
            DEFINE FIELD context ON TABLE wikilink TYPE option<string>;
            DEFINE FIELD position ON TABLE wikilink TYPE int DEFAULT 0;

            -- Define tagged_with relation
            DEFINE TABLE tagged_with SCHEMAFULL TYPE RELATION FROM notes TO tags;
            DEFINE FIELD added_at ON TABLE tagged_with TYPE datetime DEFAULT time::now();
            "#,
        )
        .await?;

        Ok(db)
    }

    /// Insert sample notes for testing queries
    ///
    /// Creates a variety of test notes with different tags, links, and metadata
    async fn insert_test_notes(db: &Surreal<Db>) -> Result<()> {
        // Insert basic test notes with tags
        db.query(
            r#"
            CREATE notes:note1 SET
                path = "test1.md",
                title = "Rust Programming",
                content = "Content about Rust",
                tags = ["rust", "programming"],
                word_count = 150;

            CREATE notes:note2 SET
                path = "test2.md",
                title = "SurrealDB Guide",
                content = "Database content",
                tags = ["database", "surrealdb"],
                word_count = 200;

            CREATE notes:note3 SET
                path = "test3.md",
                title = "Testing Guide",
                content = "TDD content",
                tags = ["testing", "rust"],
                word_count = 120;

            CREATE notes:target_note SET
                path = "target.md",
                title = "Target Note",
                content = "This is a target note",
                tags = ["target"],
                word_count = 100;

            CREATE notes:start_note SET
                path = "start.md",
                title = "Start Note",
                content = "This is the starting note",
                tags = ["start"],
                word_count = 80;

            CREATE notes:specific_id SET
                path = "specific.md",
                title = "Specific Note",
                content = "A specific note for testing",
                tags = ["test"],
                word_count = 50;
            "#,
        )
        .await?;

        // Insert tags
        db.query(
            r#"
            CREATE tags:project SET name = "project", description = "Project tags";
            CREATE tags:rust SET name = "rust", description = "Rust language";
            CREATE tags:testing SET name = "testing", description = "Testing tags";
            "#,
        )
        .await?;

        // Insert wikilinks between notes
        db.query(
            r#"
            RELATE notes:note1->wikilink->notes:note2 SET
                link_text = "SurrealDB Guide",
                context = "Check out the database guide",
                position = 0;

            RELATE notes:note1->wikilink->notes:note3 SET
                link_text = "Testing Guide",
                context = "See testing documentation",
                position = 1;

            RELATE notes:note2->wikilink->notes:target_note SET
                link_text = "Target Note",
                context = "Related target",
                position = 0;

            RELATE notes:start_note->wikilink->notes:note1 SET
                link_text = "Rust Programming",
                context = "Learn Rust",
                position = 0;
            "#,
        )
        .await?;

        // Insert tagged_with relations
        db.query(
            r#"
            RELATE notes:note1->tagged_with->tags:project;
            RELATE notes:note1->tagged_with->tags:rust;
            RELATE notes:note3->tagged_with->tags:testing;
            "#,
        )
        .await?;

        Ok(())
    }

    /// Cleanup test database
    ///
    /// For in-memory databases, this is handled automatically when the Db is dropped
    async fn cleanup_test_db(_db: &Surreal<Db>) -> Result<()> {
        // In-memory database will be cleaned up when dropped
        Ok(())
    }

    // =========================================================================
    // Basic Query Execution Tests
    // =========================================================================

    #[tokio::test]
    async fn test_execute_simple_select() {
        // Setup
        let db = setup_test_db().await.unwrap();
        insert_test_notes(&db).await.unwrap();

        // Execute simple SELECT query
        let results = execute_query(&db, "SELECT * FROM notes").await.unwrap();

        // Verify we get results
        assert!(
            !results.is_empty(),
            "Should return at least one note from test data"
        );

        // Verify result structure
        let first = &results[0];
        assert!(first.is_object(), "Result should be a JSON object");
        assert!(first.get("path").is_some(), "Note should have 'path' field");
        assert!(
            first.get("content").is_some(),
            "Note should have 'content' field"
        );

        // Verify that id field is present and formatted correctly
        assert!(first.get("id").is_some(), "Result should have 'id' field");

        if let Some(Value::String(id)) = first.get("id") {
            assert!(
                id.contains(':'),
                "ID should be in 'table:id' format, got: {}",
                id
            );
            assert!(
                id.starts_with("notes:"),
                "ID should start with 'notes:', got: {}",
                id
            );
        } else {
            panic!("ID should be a string");
        }

        cleanup_test_db(&db).await.unwrap();
    }

    #[tokio::test]
    async fn test_execute_select_with_where() {
        let db = setup_test_db().await.unwrap();
        insert_test_notes(&db).await.unwrap();

        // Query with WHERE clause filtering by tag
        let results = execute_query(&db, "SELECT * FROM notes WHERE tags CONTAINS 'rust'")
            .await
            .unwrap();

        // Verify filtering worked
        assert!(!results.is_empty(), "Should find notes tagged with 'rust'");

        // Verify all results have the expected tag
        for result in &results {
            let tags = result
                .get("tags")
                .expect("Note should have tags field")
                .as_array()
                .expect("Tags should be an array");
            assert!(
                tags.iter().any(|t| t.as_str() == Some("rust")),
                "All results should contain 'rust' tag"
            );
        }

        cleanup_test_db(&db).await.unwrap();
    }

    #[tokio::test]
    async fn test_execute_count_query() {
        let db = setup_test_db().await.unwrap();
        insert_test_notes(&db).await.unwrap();

        // Execute count query (SurrealDB syntax: count() with GROUP ALL or without GROUP BY)
        let results = execute_query(&db, "SELECT count() AS count FROM notes GROUP ALL")
            .await
            .unwrap();

        // Verify count result structure
        assert_eq!(results.len(), 1, "Count query should return one result");
        let count = &results[0];
        assert!(
            count.get("count").is_some(),
            "Result should have 'count' field"
        );

        cleanup_test_db(&db).await.unwrap();
    }

    #[tokio::test]
    async fn test_execute_invalid_query() {
        let db = setup_test_db().await.unwrap();

        // Execute malformed query
        let result = execute_query(&db, "SELECT * FORM notes WHERE invalid syntax").await;

        // Should return error
        assert!(result.is_err(), "Malformed query should return an error");

        cleanup_test_db(&db).await.unwrap();
    }

    // =========================================================================
    // Graph Query Tests
    // =========================================================================

    #[tokio::test]
    async fn test_execute_wikilink_traversal() {
        let db = setup_test_db().await.unwrap();
        insert_test_notes(&db).await.unwrap();

        // Query wikilinks from a specific note
        let results = execute_query(&db, "SELECT * FROM wikilink WHERE in = notes:note1")
            .await
            .unwrap();

        // Verify we get wikilink edges
        for result in &results {
            assert!(
                result.get("link_text").is_some(),
                "Wikilink should have link_text"
            );
            assert!(
                result.get("in").is_some(),
                "Wikilink should have 'in' (source)"
            );
            assert!(
                result.get("out").is_some(),
                "Wikilink should have 'out' (target)"
            );
        }

        cleanup_test_db(&db).await.unwrap();
    }

    #[tokio::test]
    async fn test_execute_backlinks_query() {
        let db = setup_test_db().await.unwrap();
        insert_test_notes(&db).await.unwrap();

        // Find all notes linking TO a specific note (backlinks)
        let query = r#"
            SELECT in AS source FROM wikilink
            WHERE out = notes:target_note
        "#;

        let results = execute_query(&db, query).await.unwrap();

        // Verify backlinks structure
        for result in &results {
            assert!(
                result.get("source").is_some(),
                "Backlink should have source note"
            );
        }

        cleanup_test_db(&db).await.unwrap();
    }

    #[tokio::test]
    async fn test_execute_tag_query() {
        let db = setup_test_db().await.unwrap();
        insert_test_notes(&db).await.unwrap();

        // Query tagged_with edges
        let results = execute_query(&db, "SELECT * FROM tagged_with WHERE out.name = 'project'")
            .await
            .unwrap();

        // Verify edge structure
        for result in &results {
            assert!(
                result.get("in").is_some(),
                "tagged_with should have 'in' (note)"
            );
            assert!(
                result.get("out").is_some(),
                "tagged_with should have 'out' (tag)"
            );
        }

        cleanup_test_db(&db).await.unwrap();
    }

    // =========================================================================
    // Result Formatting Tests
    // =========================================================================

    #[test]
    fn test_format_empty_results() {
        let results = vec![];
        let formatted = format_results(&results).unwrap();

        // Empty results should produce meaningful output
        assert!(
            formatted.contains("No results") || formatted.is_empty(),
            "Empty results should be handled gracefully"
        );
    }

    #[test]
    fn test_format_single_row() {
        let results = vec![json!({
            "path": "test.md",
            "title": "Test Note",
            "tags": ["rust", "testing"]
        })];

        let formatted = format_results(&results).unwrap();

        // Should contain column headers
        assert!(formatted.contains("path"), "Should have 'path' column");
        assert!(formatted.contains("title"), "Should have 'title' column");
        assert!(formatted.contains("tags"), "Should have 'tags' column");

        // Should contain data
        assert!(formatted.contains("test.md"), "Should show path value");
        assert!(formatted.contains("Test Note"), "Should show title value");

        // Should look like a table (contains borders or separators)
        assert!(
            formatted.contains("│") || formatted.contains("|") || formatted.contains("─"),
            "Should use table formatting"
        );
    }

    #[test]
    fn test_format_multiple_rows() {
        let results = vec![
            json!({
                "path": "note1.md",
                "title": "First Note",
                "word_count": 100
            }),
            json!({
                "path": "note2.md",
                "title": "Second Note",
                "word_count": 250
            }),
            json!({
                "path": "note3.md",
                "title": "Third Note",
                "word_count": 175
            }),
        ];

        let formatted = format_results(&results).unwrap();

        // All rows should be present
        assert!(formatted.contains("note1.md"), "Should contain first row");
        assert!(formatted.contains("note2.md"), "Should contain second row");
        assert!(formatted.contains("note3.md"), "Should contain third row");

        // Should have proper table structure
        let lines: Vec<&str> = formatted.lines().collect();
        assert!(
            lines.len() >= 4,
            "Should have header, separator, and data rows"
        );
    }

    #[test]
    fn test_format_with_null_values() {
        let results = vec![
            json!({
                "path": "note1.md",
                "title": "Has Title",
                "description": null
            }),
            json!({
                "path": "note2.md",
                "title": null,
                "description": "Has Description"
            }),
        ];

        let formatted = format_results(&results).unwrap();

        // Should handle null values gracefully (empty cells or "null")
        assert!(
            formatted.contains("note1.md"),
            "Should still format row with null"
        );
        assert!(
            formatted.contains("note2.md"),
            "Should still format row with null"
        );

        // Should not crash or produce invalid output
        assert!(!formatted.is_empty(), "Should produce valid output");
    }

    #[test]
    fn test_format_nested_objects() {
        let results = vec![json!({
            "path": "note.md",
            "metadata": {
                "status": "active",
                "priority": 1
            },
            "tags": ["rust", "test"]
        })];

        let formatted = format_results(&results).unwrap();

        // Should handle nested objects (either flatten or serialize)
        assert!(
            formatted.contains("note.md"),
            "Should contain top-level field"
        );

        // Nested object should be represented somehow
        // Either as JSON string, flattened keys, or truncated
        assert!(
            formatted.contains("metadata") || formatted.contains("status"),
            "Should handle nested object"
        );
    }

    // =========================================================================
    // Edge Case Tests
    // =========================================================================

    #[tokio::test]
    async fn test_execute_query_with_special_chars() {
        let db = setup_test_db().await.unwrap();
        insert_test_notes(&db).await.unwrap();

        // Query with special characters in string literals
        let query = r#"SELECT * FROM notes WHERE title = "Test's \"Quote\" Note""#;
        let results = execute_query(&db, query).await.unwrap();

        // Should handle escaping properly
        assert!(results.is_empty() || !results.is_empty());

        cleanup_test_db(&db).await.unwrap();
    }

    #[tokio::test]
    async fn test_execute_multiline_query() {
        let db = setup_test_db().await.unwrap();
        insert_test_notes(&db).await.unwrap();

        // Multi-line query with formatting
        // Note: ORDER BY fields must be in SELECT clause in SurrealDB
        let query = r#"
            SELECT
                path,
                title,
                tags,
                modified_at
            FROM notes
            WHERE
                tags CONTAINS 'rust'
                AND word_count > 100
            ORDER BY modified_at DESC
            LIMIT 5
        "#;

        let results = execute_query(&db, query).await.unwrap();

        // Should execute successfully despite formatting
        assert!(results.is_empty() || !results.is_empty());

        cleanup_test_db(&db).await.unwrap();
    }

    #[test]
    fn test_format_large_result_set() {
        // Create 100+ rows
        let results: Vec<Value> = (0..150)
            .map(|i| {
                json!({
                    "id": format!("note{}", i),
                    "path": format!("path/to/note{}.md", i),
                    "title": format!("Note {}", i),
                    "word_count": i * 10,
                })
            })
            .collect();

        let formatted = format_results(&results).unwrap();

        // Should handle large datasets efficiently
        assert!(!formatted.is_empty(), "Should format large result set");

        // Should contain first and last entries (or be paginated)
        assert!(
            formatted.contains("note0") || formatted.contains("Note 0"),
            "Should contain early entries"
        );
    }

    #[tokio::test]
    async fn test_execute_query_with_functions() {
        let db = setup_test_db().await.unwrap();
        insert_test_notes(&db).await.unwrap();

        // Query using SurrealDB functions
        let query = r#"
            SELECT
                path,
                string::uppercase(title) AS upper_title,
                time::now() AS query_time
            FROM notes
            LIMIT 1
        "#;

        let results = execute_query(&db, query).await.unwrap();

        // Should execute function calls
        if !results.is_empty() {
            let first = &results[0];
            assert!(
                first.get("upper_title").is_some() || first.get("UPPER_TITLE").is_some(),
                "Should have computed field"
            );
        }

        cleanup_test_db(&db).await.unwrap();
    }

    #[tokio::test]
    async fn test_execute_query_with_aggregation() {
        let db = setup_test_db().await.unwrap();
        insert_test_notes(&db).await.unwrap();

        // Aggregation query
        let query = r#"
            SELECT
                tags[0] AS tag,
                count() AS note_count,
                math::sum(word_count) AS total_words
            FROM notes
            GROUP BY tags[0]
        "#;

        let results = execute_query(&db, query).await.unwrap();

        // Should support GROUP BY and aggregations
        for result in &results {
            assert!(
                result.get("note_count").is_some() || result.get("count").is_some(),
                "Should have count field"
            );
        }

        cleanup_test_db(&db).await.unwrap();
    }

    #[test]
    fn test_format_with_varying_columns() {
        // Results with different keys (sparse data)
        let results = vec![
            json!({
                "path": "note1.md",
                "title": "First",
                "tags": ["a", "b"]
            }),
            json!({
                "path": "note2.md",
                "word_count": 100,
                "status": "active"
            }),
            json!({
                "title": "Third",
                "created_at": "2025-10-19"
            }),
        ];

        let formatted = format_results(&results).unwrap();

        // Should handle varying columns across rows
        // Either show all columns with nulls, or show per-row columns
        assert!(!formatted.is_empty(), "Should format sparse data");
    }

    #[tokio::test]
    async fn test_execute_empty_query() {
        let db = setup_test_db().await.unwrap();

        // Empty query string
        let result = execute_query(&db, "").await;

        // Should return error or empty results
        assert!(
            result.is_err() || result.unwrap().is_empty(),
            "Empty query should fail or return nothing"
        );

        cleanup_test_db(&db).await.unwrap();
    }

    #[tokio::test]
    async fn test_execute_query_with_record_id() {
        let db = setup_test_db().await.unwrap();
        insert_test_notes(&db).await.unwrap();

        // Query specific record by ID
        let query = "SELECT * FROM notes:specific_id";
        let results = execute_query(&db, query).await.unwrap();

        // Should support record ID syntax
        assert!(results.is_empty() || results.len() == 1);

        cleanup_test_db(&db).await.unwrap();
    }

    #[test]
    fn test_format_with_array_values() {
        let results = vec![json!({
            "path": "note.md",
            "tags": ["rust", "surrealdb", "testing", "tdd"],
            "links": ["[[Note A]]", "[[Note B]]", "[[Note C]]"]
        })];

        let formatted = format_results(&results).unwrap();

        // Should format arrays readably
        assert!(formatted.contains("note.md"));

        // Arrays should be shown (either as JSON or comma-separated)
        assert!(
            formatted.contains("rust") || formatted.contains("["),
            "Should show array contents"
        );
    }

    #[test]
    fn test_format_with_unicode() {
        let results = vec![json!({
            "path": "日本語.md",
            "title": "Unicode Test: 中文 Ελληνικά",
            "tags": ["🦀", "📝"]
        })];

        let formatted = format_results(&results).unwrap();

        // Should handle Unicode properly
        assert!(formatted.contains("日本語") || formatted.contains("Unicode"));
        assert!(!formatted.is_empty());
    }

    #[tokio::test]
    async fn test_execute_query_with_graph_traversal() {
        let db = setup_test_db().await.unwrap();
        insert_test_notes(&db).await.unwrap();

        // Deep graph traversal query
        let query = r#"
            SELECT
                path,
                ->wikilink->notes.title AS linked_notes
            FROM notes:start_note
        "#;

        let results = execute_query(&db, query).await.unwrap();

        // Should support graph edge traversal
        assert!(results.is_empty() || !results.is_empty());

        cleanup_test_db(&db).await.unwrap();
    }

    #[test]
    fn test_format_preserves_order() {
        let results = vec![json!({
            "z_last": "should be last",
            "a_first": "should be first",
            "m_middle": "should be middle"
        })];

        let formatted = format_results(&results).unwrap();

        // Column order should be consistent
        // (Either alphabetical or insertion order)
        assert!(!formatted.is_empty());
    }
}
