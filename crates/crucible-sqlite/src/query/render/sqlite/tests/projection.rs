use crate::query::ir::{
    EdgeDirection, EdgePattern, GraphIR, GraphPattern, NodePattern, PatternElement, Projection,
    QuerySource,
};
use crate::query::render::sqlite::SqliteRenderer;
use crate::query::render::QueryRenderer;

// =========================================================================
// Projection tests
// =========================================================================

#[test]
fn test_render_with_projections() {
    let renderer = SqliteRenderer::default();
    let ir = GraphIR {
        source: QuerySource::All,
        projections: vec![
            Projection {
                field: "path".to_string(),
                alias: None,
            },
            Projection {
                field: "title".to_string(),
                alias: Some("name".to_string()),
            },
        ],
        ..Default::default()
    };

    let result = renderer.render(&ir).unwrap();

    assert!(result.sql.contains("SELECT path, title AS name"));
}

// =========================================================================
// Custom table names
// =========================================================================

#[test]
fn test_custom_tables() {
    let renderer = SqliteRenderer::with_tables("documents", "links");
    let ir = GraphIR::default();

    let result = renderer.render(&ir).unwrap();

    assert!(result.sql.contains("FROM documents"));
}

// =========================================================================
// Custom column names
// =========================================================================

#[test]
fn test_custom_columns_with_schema() {
    let renderer = SqliteRenderer::with_schema("nodes", "edges", "src", "dst", "edge_kind");
    let ir = GraphIR {
        source: QuerySource::ByTitle("Test".to_string()),
        pattern: GraphPattern {
            elements: vec![
                PatternElement::Node(NodePattern {
                    alias: Some("a".to_string()),
                    ..Default::default()
                }),
                PatternElement::Edge(EdgePattern {
                    direction: EdgeDirection::Out,
                    edge_type: Some("link".to_string()),
                    ..Default::default()
                }),
                PatternElement::Node(NodePattern {
                    alias: Some("b".to_string()),
                    ..Default::default()
                }),
            ],
        },
        ..Default::default()
    };

    let result = renderer.render(&ir).unwrap();

    assert!(result.sql.contains("FROM nodes"));
    assert!(result.sql.contains("JOIN edges"));
    assert!(result.sql.contains("e0.src"));
    assert!(result.sql.contains("e0.edge_kind"));
}
