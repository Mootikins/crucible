use crate::storage::sqlite::query::ir::{GraphIR, QuerySource};
use crate::storage::sqlite::query::render::sqlite::SqliteRenderer;
use crate::storage::sqlite::query::render::QueryRenderer;
use serde_json::Value;

// =========================================================================
// Simple lookup tests
// =========================================================================

#[test]
fn test_render_select_all() {
    let renderer = SqliteRenderer::default();
    let ir = GraphIR::default();

    let result = renderer.render(&ir).unwrap();

    assert_eq!(result.sql, "SELECT * FROM notes");
}

#[test]
fn test_render_find_by_title() {
    let renderer = SqliteRenderer::default();
    let ir = GraphIR {
        source: QuerySource::ByTitle("Index".to_string()),
        ..Default::default()
    };

    let result = renderer.render(&ir).unwrap();

    assert!(result.sql.contains("WHERE title = :title"));
    assert_eq!(
        result.params.get("title"),
        Some(&Value::String("Index".to_string()))
    );
}

#[test]
fn test_render_find_by_path() {
    let renderer = SqliteRenderer::default();
    let ir = GraphIR {
        source: QuerySource::ByPath("notes/index.md".to_string()),
        ..Default::default()
    };

    let result = renderer.render(&ir).unwrap();

    assert!(result.sql.contains("WHERE path = :path"));
}

// Snapshot tests (bare-named)

#[test]
fn select_all() {
    let renderer = SqliteRenderer::default();
    let ir = GraphIR::default();
    let result = renderer.render(&ir).unwrap();
    insta::assert_snapshot!(result.sql);
}

#[test]
fn find_by_title() {
    let renderer = SqliteRenderer::default();
    let ir = GraphIR {
        source: QuerySource::ByTitle("Index".to_string()),
        ..Default::default()
    };
    let result = renderer.render(&ir).unwrap();
    insta::assert_snapshot!(result.sql);
}

#[test]
fn find_by_path() {
    let renderer = SqliteRenderer::default();
    let ir = GraphIR {
        source: QuerySource::ByPath("notes/index.md".to_string()),
        ..Default::default()
    };
    let result = renderer.render(&ir).unwrap();
    insta::assert_snapshot!(result.sql);
}
