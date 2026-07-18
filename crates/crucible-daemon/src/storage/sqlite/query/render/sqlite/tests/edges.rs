use crate::storage::sqlite::query::ir::{
    EdgeDirection, EdgePattern, GraphIR, GraphPattern, NodePattern, PatternElement, QuerySource,
};
use crate::storage::sqlite::query::render::sqlite::SqliteRenderer;
use crate::storage::sqlite::query::render::QueryRenderer;
use serde_json::Value;

// =========================================================================
// Simple edge pattern tests
// =========================================================================

#[test]
fn test_render_outlinks() {
    let renderer = SqliteRenderer::default();
    let ir = GraphIR {
        source: QuerySource::ByTitle("Index".to_string()),
        pattern: GraphPattern {
            elements: vec![
                PatternElement::Node(NodePattern {
                    alias: Some("a".to_string()),
                    ..Default::default()
                }),
                PatternElement::Edge(EdgePattern {
                    direction: EdgeDirection::Out,
                    edge_type: Some("wikilink".to_string()),
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

    assert!(result.sql.contains("SELECT a.*"));
    assert!(result.sql.contains("JOIN edges e0 ON e0.source = a.path"));
    assert!(result.sql.contains("e0.type = :edge_type_0"));
    assert_eq!(
        result.params.get("edge_type_0"),
        Some(&Value::String("wikilink".to_string()))
    );
}

#[test]
fn test_render_inlinks() {
    let renderer = SqliteRenderer::default();
    let ir = GraphIR {
        source: QuerySource::ByTitle("Project".to_string()),
        pattern: GraphPattern {
            elements: vec![
                PatternElement::Node(NodePattern {
                    alias: Some("a".to_string()),
                    ..Default::default()
                }),
                PatternElement::Edge(EdgePattern {
                    direction: EdgeDirection::In,
                    edge_type: Some("wikilink".to_string()),
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

    assert!(result.sql.contains("JOIN edges e0 ON e0.target = a.path"));
}

#[test]
fn test_render_bidirectional() {
    let renderer = SqliteRenderer::default();
    let ir = GraphIR {
        source: QuerySource::ByTitle("Hub".to_string()),
        pattern: GraphPattern {
            elements: vec![
                PatternElement::Node(NodePattern {
                    alias: Some("a".to_string()),
                    ..Default::default()
                }),
                PatternElement::Edge(EdgePattern {
                    direction: EdgeDirection::Both,
                    edge_type: Some("wikilink".to_string()),
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

    assert!(result
        .sql
        .contains("e0.source = a.path OR e0.target = a.path"));
}

// Snapshot tests (bare-named)

#[test]
fn outlinks() {
    let renderer = SqliteRenderer::default();
    let ir = GraphIR {
        source: QuerySource::ByTitle("Index".to_string()),
        pattern: GraphPattern {
            elements: vec![
                PatternElement::Node(NodePattern {
                    alias: Some("a".to_string()),
                    ..Default::default()
                }),
                PatternElement::Edge(EdgePattern {
                    direction: EdgeDirection::Out,
                    edge_type: Some("wikilink".to_string()),
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
    insta::assert_snapshot!(result.sql);
}

#[test]
fn inlinks() {
    let renderer = SqliteRenderer::default();
    let ir = GraphIR {
        source: QuerySource::ByTitle("Project".to_string()),
        pattern: GraphPattern {
            elements: vec![
                PatternElement::Node(NodePattern {
                    alias: Some("a".to_string()),
                    ..Default::default()
                }),
                PatternElement::Edge(EdgePattern {
                    direction: EdgeDirection::In,
                    edge_type: Some("wikilink".to_string()),
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
    insta::assert_snapshot!(result.sql);
}

#[test]
fn bidirectional() {
    let renderer = SqliteRenderer::default();
    let ir = GraphIR {
        source: QuerySource::ByTitle("Hub".to_string()),
        pattern: GraphPattern {
            elements: vec![
                PatternElement::Node(NodePattern {
                    alias: Some("a".to_string()),
                    ..Default::default()
                }),
                PatternElement::Edge(EdgePattern {
                    direction: EdgeDirection::Both,
                    edge_type: Some("wikilink".to_string()),
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
    insta::assert_snapshot!(result.sql);
}
