//! Graph query translation via composable pipeline.
//!
//! This module provides factory functions for creating query pipelines that
//! translate multiple query syntaxes into SurrealQL for graph traversal.
//!
//! # Supported Syntaxes
//!
//! The pipeline supports three query syntaxes with priority-based selection:
//!
//! | Priority | Syntax | Example |
//! |----------|--------|---------|
//! | 50 | SQL/PGQ MATCH | `MATCH (a {title: 'X'})-[:wikilink]->(b)` |
//! | 40 | SQL Sugar | `SELECT outlinks FROM 'Title'` |
//! | 30 | jaq-style | `outlinks("Title")` |
//!
//! # Graph Functions (jaq-style)
//!
//! - `outlinks("title")` - Notes linked FROM the given note
//! - `inlinks("title")` - Notes linking TO the given note (backlinks)
//! - `find("title")` - Find a note by title
//! - `neighbors("title")` - All connected notes (outlinks + inlinks)
//!
//! # Example
//!
//! ```ignore
//! use crucible_surrealdb::graph_query::create_default_pipeline;
//!
//! let pipeline = create_default_pipeline();
//!
//! // Any syntax works - pipeline auto-detects
//! let result = pipeline.execute(r#"outlinks("Index")"#)?;
//! let result = pipeline.execute("SELECT outlinks FROM 'Index'")?;
//! let result = pipeline.execute("MATCH (a {title: 'Index'})-[:wikilink]->(b)")?;
//!
//! println!("SQL: {}", result.sql);
//! println!("Params: {:?}", result.params);
//! ```

use crucible_query::{
    render::SurrealRenderer,
    syntax::JaqSyntax,
    syntax::PgqSyntax,
    syntax::QuerySyntaxRegistryBuilder,
    syntax::SqlSugarSyntax,
    transform::{FilterTransform, ValidateTransform},
    QueryPipeline, QueryPipelineBuilder,
};

// ============================================================================
// Pipeline Factory
// ============================================================================

/// Create the default Crucible query pipeline.
///
/// This pipeline supports multiple query syntaxes:
/// - SQL/PGQ MATCH (priority 50): `MATCH (a {title: 'X'})-[:wikilink]->(b)`
/// - SQL sugar (priority 40): `SELECT outlinks FROM 'Title'`
/// - jaq-style (priority 30): `outlinks("Title")`
///
/// And renders to SurrealQL for execution.
pub fn create_default_pipeline() -> QueryPipeline {
    create_pipeline_with_tables("entities", "relations")
}

/// Create a query pipeline with custom table names.
///
/// # Arguments
///
/// * `entity_table` - Table name for entities (notes)
/// * `relation_table` - Table name for relations (wikilinks)
pub fn create_pipeline_with_tables(
    entity_table: impl Into<String>,
    relation_table: impl Into<String>,
) -> QueryPipeline {
    let syntax_registry = QuerySyntaxRegistryBuilder::new()
        .with_syntax(PgqSyntax) // Priority 50 - SQL/PGQ MATCH
        .with_syntax(SqlSugarSyntax) // Priority 40
        .with_syntax(JaqSyntax) // Priority 30
        .build();

    QueryPipelineBuilder::new()
        .syntax_registry(syntax_registry)
        .transform(ValidateTransform)
        .transform(FilterTransform)
        .renderer(SurrealRenderer::with_tables(entity_table, relation_table))
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    // =========================================================================
    // SQL Sugar syntax tests
    // =========================================================================

    #[test]
    fn test_pipeline_sql_sugar_outlinks() {
        let pipeline = create_default_pipeline();
        let result = pipeline.execute("SELECT outlinks FROM 'Index'").unwrap();

        assert!(result.sql.contains("SELECT"));
        assert!(result.sql.contains("FETCH out"));
        assert_eq!(
            result.params.get("title"),
            Some(&Value::String("Index".to_string()))
        );
    }

    #[test]
    fn test_pipeline_sql_sugar_inlinks() {
        let pipeline = create_default_pipeline();
        let result = pipeline.execute("SELECT inlinks FROM 'Project'").unwrap();

        assert!(result.sql.contains("FETCH `in`"));
        assert_eq!(
            result.params.get("title"),
            Some(&Value::String("Project".to_string()))
        );
    }

    #[test]
    fn test_pipeline_sql_sugar_neighbors() {
        let pipeline = create_default_pipeline();
        let result = pipeline.execute("SELECT neighbors FROM 'Hub'").unwrap();

        assert!(result.sql.contains("array::concat"));
    }

    #[test]
    fn test_pipeline_sql_sugar_find() {
        let pipeline = create_default_pipeline();
        let result = pipeline
            .execute("SELECT * FROM notes WHERE title = 'MyNote'")
            .unwrap();

        assert!(result.sql.contains("SELECT * FROM entities"));
        assert!(result.sql.contains("WHERE title = $title"));
    }

    // =========================================================================
    // jaq-style syntax tests
    // =========================================================================

    #[test]
    fn test_pipeline_jaq_outlinks() {
        let pipeline = create_default_pipeline();
        let result = pipeline.execute(r#"outlinks("Index")"#).unwrap();

        assert!(result.sql.contains("SELECT"));
        assert!(result.sql.contains("FETCH out"));
        assert_eq!(
            result.params.get("title"),
            Some(&Value::String("Index".to_string()))
        );
    }

    #[test]
    fn test_pipeline_jaq_find() {
        let pipeline = create_default_pipeline();
        let result = pipeline.execute(r#"find("MyNote")"#).unwrap();

        assert!(result.sql.contains("SELECT * FROM entities"));
        assert!(result.sql.contains("WHERE title = $title"));
    }

    // =========================================================================
    // Custom table names
    // =========================================================================

    #[test]
    fn test_pipeline_custom_tables() {
        let pipeline = create_pipeline_with_tables("notes", "wikilinks");
        let result = pipeline
            .execute("SELECT * FROM notes WHERE title = 'Test'")
            .unwrap();

        assert!(result.sql.contains("FROM notes"));
    }
}
