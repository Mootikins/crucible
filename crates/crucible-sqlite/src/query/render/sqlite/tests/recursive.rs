use crate::query::error::RenderError;
use crate::query::ir::{
    EdgeDirection, EdgePattern, GraphIR, GraphPattern, NodePattern, PatternElement, Quantifier,
    QuerySource,
};
use crate::query::render::sqlite::SqliteRenderer;
use crate::query::render::QueryRenderer;
use serde_json::Value;

// =========================================================================
// Recursive query tests
// =========================================================================

#[test]
fn test_render_variable_length_path() {
    let renderer = SqliteRenderer::default();
    let ir = GraphIR {
        source: QuerySource::ByPath("index.md".to_string()),
        pattern: GraphPattern {
            elements: vec![
                PatternElement::Node(NodePattern {
                    alias: Some("a".to_string()),
                    ..Default::default()
                }),
                PatternElement::Edge(EdgePattern {
                    direction: EdgeDirection::Out,
                    edge_type: Some("LINKS_TO".to_string()),
                    quantifier: Some(Quantifier::Range {
                        min: 1,
                        max: Some(3),
                    }),
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

    assert!(result.sql.contains("WITH RECURSIVE traverse"));
    assert!(result.sql.contains("t.depth < 3"));
    assert!(result.sql.contains("t.depth >= 1"));
    assert!(result.sql.contains("e.type = :edge_type"));
    assert_eq!(
        result.params.get("edge_type"),
        Some(&Value::String("LINKS_TO".to_string()))
    );
}

#[test]
fn test_render_star_quantifier() {
    let renderer = SqliteRenderer::default();
    let ir = GraphIR {
        source: QuerySource::ByPath("index.md".to_string()),
        pattern: GraphPattern {
            elements: vec![
                PatternElement::Node(NodePattern::default()),
                PatternElement::Edge(EdgePattern {
                    direction: EdgeDirection::Out,
                    quantifier: Some(Quantifier::ZeroOrMore),
                    ..Default::default()
                }),
                PatternElement::Node(NodePattern::default()),
            ],
        },
        ..Default::default()
    };

    let result = renderer.render(&ir).unwrap();

    assert!(result.sql.contains("WITH RECURSIVE traverse"));
    assert!(result.sql.contains("t.depth >= 0"));
    // ZeroOrMore includes source as valid 0-hop match
    assert!(
        !result.sql.contains("t.path != "),
        "ZeroOrMore should not exclude source node"
    );
}

#[test]
fn test_render_plus_quantifier() {
    let renderer = SqliteRenderer::default();
    let ir = GraphIR {
        source: QuerySource::ByPath("index.md".to_string()),
        pattern: GraphPattern {
            elements: vec![
                PatternElement::Node(NodePattern::default()),
                PatternElement::Edge(EdgePattern {
                    direction: EdgeDirection::Out,
                    quantifier: Some(Quantifier::OneOrMore),
                    ..Default::default()
                }),
                PatternElement::Node(NodePattern::default()),
            ],
        },
        ..Default::default()
    };

    let result = renderer.render(&ir).unwrap();

    assert!(result.sql.contains("WITH RECURSIVE traverse"));
    assert!(result.sql.contains("t.depth >= 1"));
    // OneOrMore excludes source since we want actual traversals
    assert!(
        result.sql.contains("t.path != "),
        "OneOrMore should exclude source node"
    );
}

#[test]
fn test_recursive_requires_source() {
    let renderer = SqliteRenderer::default();
    let ir = GraphIR {
        source: QuerySource::All, // No explicit source
        pattern: GraphPattern {
            elements: vec![
                PatternElement::Node(NodePattern::default()),
                PatternElement::Edge(EdgePattern {
                    quantifier: Some(Quantifier::ZeroOrMore),
                    ..Default::default()
                }),
                PatternElement::Node(NodePattern::default()),
            ],
        },
        ..Default::default()
    };

    let result = renderer.render(&ir);

    assert!(matches!(result, Err(RenderError::MissingSource)));
}

// Snapshot tests (bare-named)

#[test]
fn recursive_range() {
    let renderer = SqliteRenderer::default();
    let ir = GraphIR {
        source: QuerySource::ByPath("index.md".to_string()),
        pattern: GraphPattern {
            elements: vec![
                PatternElement::Node(NodePattern {
                    alias: Some("a".to_string()),
                    ..Default::default()
                }),
                PatternElement::Edge(EdgePattern {
                    direction: EdgeDirection::Out,
                    edge_type: Some("LINKS_TO".to_string()),
                    quantifier: Some(Quantifier::Range {
                        min: 1,
                        max: Some(3),
                    }),
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
fn recursive_zero_or_more() {
    let renderer = SqliteRenderer::default();
    let ir = GraphIR {
        source: QuerySource::ByPath("index.md".to_string()),
        pattern: GraphPattern {
            elements: vec![
                PatternElement::Node(NodePattern::default()),
                PatternElement::Edge(EdgePattern {
                    direction: EdgeDirection::Out,
                    quantifier: Some(Quantifier::ZeroOrMore),
                    ..Default::default()
                }),
                PatternElement::Node(NodePattern::default()),
            ],
        },
        ..Default::default()
    };
    let result = renderer.render(&ir).unwrap();
    insta::assert_snapshot!(result.sql);
}

#[test]
fn recursive_one_or_more() {
    let renderer = SqliteRenderer::default();
    let ir = GraphIR {
        source: QuerySource::ByPath("index.md".to_string()),
        pattern: GraphPattern {
            elements: vec![
                PatternElement::Node(NodePattern::default()),
                PatternElement::Edge(EdgePattern {
                    direction: EdgeDirection::Out,
                    quantifier: Some(Quantifier::OneOrMore),
                    ..Default::default()
                }),
                PatternElement::Node(NodePattern::default()),
            ],
        },
        ..Default::default()
    };
    let result = renderer.render(&ir).unwrap();
    insta::assert_snapshot!(result.sql);
}
