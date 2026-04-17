use crate::query::error::RenderError;
use crate::query::ir::{Filter, GraphIR, MatchOp, QuerySource};
use crate::query::render::sqlite::SqliteRenderer;
use crate::query::render::QueryRenderer;
use serde_json::Value;

// =========================================================================
// Filter tests
// =========================================================================

#[test]
fn test_render_with_filter() {
    let renderer = SqliteRenderer::default();
    let ir = GraphIR {
        source: QuerySource::All,
        filters: vec![Filter {
            field: "folder".to_string(),
            op: MatchOp::Eq,
            value: Value::String("Projects".to_string()),
        }],
        ..Default::default()
    };

    let result = renderer.render(&ir).unwrap();

    assert!(result.sql.contains("folder = :filter_0"));
    assert_eq!(
        result.params.get("filter_0"),
        Some(&Value::String("Projects".to_string()))
    );
}

#[test]
fn test_render_with_contains_filter() {
    let renderer = SqliteRenderer::default();
    let ir = GraphIR {
        source: QuerySource::All,
        filters: vec![Filter {
            field: "title".to_string(),
            op: MatchOp::Contains,
            value: Value::String("API".to_string()),
        }],
        ..Default::default()
    };

    let result = renderer.render(&ir).unwrap();

    assert!(result.sql.contains("title LIKE :filter_0"));
    assert_eq!(
        result.params.get("filter_0"),
        Some(&Value::String("%API%".to_string()))
    );
}

#[test]
fn test_render_with_starts_with_filter() {
    let renderer = SqliteRenderer::default();
    let ir = GraphIR {
        source: QuerySource::All,
        filters: vec![Filter {
            field: "path".to_string(),
            op: MatchOp::StartsWith,
            value: Value::String("docs/".to_string()),
        }],
        ..Default::default()
    };

    let result = renderer.render(&ir).unwrap();

    assert!(result.sql.contains("path LIKE :filter_0"));
    assert_eq!(
        result.params.get("filter_0"),
        Some(&Value::String("docs/%".to_string()))
    );
}

#[test]
fn test_render_contains_with_non_string_fails() {
    let renderer = SqliteRenderer::default();
    let ir = GraphIR {
        source: QuerySource::All,
        filters: vec![Filter {
            field: "count".to_string(),
            op: MatchOp::Contains,
            value: Value::Number(42.into()),
        }],
        ..Default::default()
    };

    let result = renderer.render(&ir);
    assert!(matches!(result, Err(RenderError::UnsupportedFilter { .. })));
}

#[test]
fn test_render_starts_with_non_string_fails() {
    let renderer = SqliteRenderer::default();
    let ir = GraphIR {
        source: QuerySource::All,
        filters: vec![Filter {
            field: "status".to_string(),
            op: MatchOp::StartsWith,
            value: Value::Bool(true),
        }],
        ..Default::default()
    };

    let result = renderer.render(&ir);
    assert!(matches!(result, Err(RenderError::UnsupportedFilter { .. })));
}

// Snapshot tests (bare-named)

#[test]
fn filter_eq() {
    let renderer = SqliteRenderer::default();
    let ir = GraphIR {
        source: QuerySource::All,
        filters: vec![Filter {
            field: "folder".to_string(),
            op: MatchOp::Eq,
            value: Value::String("Projects".to_string()),
        }],
        ..Default::default()
    };
    let result = renderer.render(&ir).unwrap();
    insta::assert_snapshot!(result.sql);
}

#[test]
fn filter_contains() {
    let renderer = SqliteRenderer::default();
    let ir = GraphIR {
        source: QuerySource::All,
        filters: vec![Filter {
            field: "title".to_string(),
            op: MatchOp::Contains,
            value: Value::String("API".to_string()),
        }],
        ..Default::default()
    };
    let result = renderer.render(&ir).unwrap();
    insta::assert_snapshot!(result.sql);
}

#[test]
fn filter_starts_with() {
    let renderer = SqliteRenderer::default();
    let ir = GraphIR {
        source: QuerySource::All,
        filters: vec![Filter {
            field: "path".to_string(),
            op: MatchOp::StartsWith,
            value: Value::String("docs/".to_string()),
        }],
        ..Default::default()
    };
    let result = renderer.render(&ir).unwrap();
    insta::assert_snapshot!(result.sql);
}

#[test]
fn filter_ends_with() {
    let renderer = SqliteRenderer::default();
    let ir = GraphIR {
        source: QuerySource::All,
        filters: vec![Filter {
            field: "path".to_string(),
            op: MatchOp::EndsWith,
            value: Value::String(".md".to_string()),
        }],
        ..Default::default()
    };
    let result = renderer.render(&ir).unwrap();
    insta::assert_snapshot!(result.sql);
}
