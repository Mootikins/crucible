use crate::query::ir::{
    EdgeDirection, EdgePattern, GraphIR, GraphPattern, NodePattern, PatternElement, Quantifier,
    QuerySource,
};
use crate::query::render::sqlite::SqliteRenderer;
use crate::query::render::QueryRenderer;

#[test]
fn test_crucible_eav_schema() {
    let renderer = SqliteRenderer::for_crucible_eav();
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

    assert!(result.sql.contains("FROM entities"));
    assert!(result.sql.contains("JOIN relations"));
    assert!(result.sql.contains("from_entity_id"));
    assert!(result.sql.contains("relation_type"));
}

#[test]
fn test_crucible_eav_recursive() {
    let renderer = SqliteRenderer::for_crucible_eav();
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
                    edge_type: Some("wikilink".to_string()),
                    quantifier: Some(Quantifier::OneOrMore),
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

    // Verify recursive CTE uses custom column names
    assert!(result.sql.contains("WITH RECURSIVE traverse"));
    assert!(result.sql.contains("JOIN relations"));
    assert!(result.sql.contains("e.from_entity_id"));
    assert!(result.sql.contains("e.to_entity_id"));
    assert!(result.sql.contains("e.relation_type"));
}

// Snapshot tests (bare-named)

#[test]
fn crucible_eav_schema() {
    let renderer = SqliteRenderer::for_crucible_eav();
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

// =========================================================================
// Pipeline snapshot tests (SQL sugar → IR → SQLite)
// =========================================================================

#[test]
fn pipeline_sql_sugar_outlinks() {
    use crate::query::pipeline::QueryPipelineBuilder;
    use crate::query::render::SqliteRenderer;
    use crate::query::syntax::{QuerySyntaxRegistryBuilder, SqlSugarSyntax};
    use crate::query::transform::ValidateTransform;

    let syntax_registry = QuerySyntaxRegistryBuilder::new()
        .with_syntax(SqlSugarSyntax)
        .build();

    let pipeline = QueryPipelineBuilder::new()
        .syntax_registry(syntax_registry)
        .transform(ValidateTransform)
        .renderer(SqliteRenderer::default())
        .build();

    let result = pipeline.execute("SELECT outlinks FROM 'Index'").unwrap();
    insta::assert_snapshot!(result.sql);
}

#[test]
fn pipeline_sql_sugar_inlinks() {
    use crate::query::pipeline::QueryPipelineBuilder;
    use crate::query::render::SqliteRenderer;
    use crate::query::syntax::{QuerySyntaxRegistryBuilder, SqlSugarSyntax};
    use crate::query::transform::ValidateTransform;

    let syntax_registry = QuerySyntaxRegistryBuilder::new()
        .with_syntax(SqlSugarSyntax)
        .build();

    let pipeline = QueryPipelineBuilder::new()
        .syntax_registry(syntax_registry)
        .transform(ValidateTransform)
        .renderer(SqliteRenderer::default())
        .build();

    let result = pipeline.execute("SELECT inlinks FROM 'Project'").unwrap();
    insta::assert_snapshot!(result.sql);
}

#[test]
fn pipeline_sql_sugar_neighbors() {
    use crate::query::pipeline::QueryPipelineBuilder;
    use crate::query::render::SqliteRenderer;
    use crate::query::syntax::{QuerySyntaxRegistryBuilder, SqlSugarSyntax};
    use crate::query::transform::ValidateTransform;

    let syntax_registry = QuerySyntaxRegistryBuilder::new()
        .with_syntax(SqlSugarSyntax)
        .build();

    let pipeline = QueryPipelineBuilder::new()
        .syntax_registry(syntax_registry)
        .transform(ValidateTransform)
        .renderer(SqliteRenderer::default())
        .build();

    let result = pipeline.execute("SELECT neighbors FROM 'Hub'").unwrap();
    insta::assert_snapshot!(result.sql);
}
