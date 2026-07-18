//! SQLite query renderer.
//!
//! Renders GraphIR to SQLite SQL with:
//! - Recursive CTEs for variable-length paths
//! - JOINs for fixed-length patterns
//! - Parameter binding for values

use crate::storage::sqlite::query::error::RenderError;
use crate::storage::sqlite::query::ir::GraphIR;
use crate::storage::sqlite::query::render::{QueryRenderer, RenderedQuery};
use std::collections::HashMap;

mod filter;
mod path;
mod projection;
mod recursive;
mod simple;

#[cfg(test)]
mod tests;

/// SQLite renderer with configurable table and column names.
///
/// Assumes the following schema by default:
/// ```sql
/// CREATE TABLE notes (
///     path TEXT PRIMARY KEY,
///     title TEXT,
///     content TEXT,
///     file_hash TEXT NOT NULL
/// );
///
/// CREATE TABLE edges (
///     source TEXT NOT NULL,
///     target TEXT NOT NULL,
///     type TEXT NOT NULL,
///     PRIMARY KEY (source, target, type)
/// );
/// ```
///
/// Column names can be customized via [`Self::with_schema`] or [`Self::for_crucible_eav`].
pub struct SqliteRenderer {
    /// Table name for notes
    pub notes_table: String,
    /// Table name for edges
    pub edges_table: String,
    /// Column name for source entity (default: "source")
    pub source_column: String,
    /// Column name for target entity (default: "target")
    pub target_column: String,
    /// Column name for edge type (default: "type")
    pub type_column: String,
}

impl Default for SqliteRenderer {
    fn default() -> Self {
        Self {
            notes_table: "notes".to_string(),
            edges_table: "edges".to_string(),
            source_column: "source".to_string(),
            target_column: "target".to_string(),
            type_column: "type".to_string(),
        }
    }
}

impl SqliteRenderer {
    /// Create renderer with custom table names (uses default column names)
    pub fn with_tables(notes: impl Into<String>, edges: impl Into<String>) -> Self {
        Self {
            notes_table: notes.into(),
            edges_table: edges.into(),
            source_column: "source".to_string(),
            target_column: "target".to_string(),
            type_column: "type".to_string(),
        }
    }

    /// Create renderer with custom table and column names
    pub fn with_schema(
        notes_table: impl Into<String>,
        edges_table: impl Into<String>,
        source_column: impl Into<String>,
        target_column: impl Into<String>,
        type_column: impl Into<String>,
    ) -> Self {
        Self {
            notes_table: notes_table.into(),
            edges_table: edges_table.into(),
            source_column: source_column.into(),
            target_column: target_column.into(),
            type_column: type_column.into(),
        }
    }

    /// Create renderer for Crucible's EAV schema
    pub fn for_crucible_eav() -> Self {
        Self::with_schema(
            "entities",
            "relations",
            "from_entity_id",
            "to_entity_id",
            "relation_type",
        )
    }
}

impl QueryRenderer for SqliteRenderer {
    fn name(&self) -> &str {
        "sqlite"
    }

    fn render(&self, ir: &GraphIR) -> Result<RenderedQuery, RenderError> {
        let mut params = HashMap::new();

        // Handle empty pattern (simple lookup)
        if ir.pattern.elements.is_empty() {
            return self.render_simple_lookup(ir, &mut params);
        }

        // Check if we need recursive CTE
        let sql = if self.needs_recursion(ir) {
            self.render_recursive(ir, &mut params)?
        } else {
            self.render_simple(ir, &mut params)?
        };

        Ok(RenderedQuery { sql, params })
    }
}
