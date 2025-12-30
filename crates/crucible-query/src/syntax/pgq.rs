//! SQL/PGQ MATCH syntax parser using chumsky.
//!
//! Parses SQL:2023 graph pattern matching queries:
//! - `MATCH (a {title: 'X'})-[:wikilink]->(b)`
//! - `MATCH (a)<-[:wikilink]-(b {title: 'Y'})`
//!
//! Priority: 50 (default, preferred for LLMs)

use crate::error::ParseError;
use crate::ir::{
    EdgeDirection, EdgePattern, GraphIR, GraphPattern, MatchOp, NodePattern, PatternElement,
    PropertyMatch, QuerySource,
};
use crate::syntax::QuerySyntax;
use chumsky::prelude::*;
use chumsky::extra;
use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::Value;

/// Extra type for our parsers - uses Rich errors for better messages
type Extra<'src> = extra::Err<Rich<'src, char>>;

/// Fast prefix check for MATCH or FROM GRAPH
static MATCH_PREFIX_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)^\s*(MATCH|FROM\s+GRAPH)").unwrap());

/// SQL/PGQ MATCH syntax parser
pub struct PgqSyntax;

impl QuerySyntax for PgqSyntax {
    fn name(&self) -> &'static str {
        "sql-pgq"
    }

    fn can_handle(&self, input: &str) -> bool {
        MATCH_PREFIX_RE.is_match(input)
    }

    fn parse(&self, input: &str) -> Result<GraphIR, ParseError> {
        match_query_parser()
            .parse(input)
            .into_result()
            .map_err(|errs| ParseError::Pgq {
                errors: format_chumsky_errors(&errs, input),
            })
    }

    fn priority(&self) -> u8 {
        50 // Default priority
    }
}

// ============================================================================
// Chumsky parsers
// ============================================================================

/// Main parser for MATCH queries
fn match_query_parser<'src>() -> impl Parser<'src, &'src str, GraphIR, Extra<'src>> {
    // MATCH keyword (case insensitive)
    let match_kw = choice((
        text::keyword::<&str, _, Extra<'src>>("MATCH"),
        text::keyword::<&str, _, Extra<'src>>("match"),
        text::keyword::<&str, _, Extra<'src>>("Match"),
    ))
    .padded();

    // Full MATCH pattern (.parse() consumes all input in 0.12)
    match_kw
        .ignore_then(graph_pattern_parser())
        .map(build_graph_ir)
        .padded()
}

/// Parser for graph pattern: (node)-[edge]->(node)
fn graph_pattern_parser<'src>() -> impl Parser<'src, &'src str, Vec<PatternPart>, Extra<'src>> {
    // A pattern is: node (edge node)*
    node_parser()
        .then(edge_then_node_parser().repeated().collect::<Vec<_>>())
        .map(|(first, rest)| {
            let mut parts = vec![PatternPart::Node(first)];
            for (edge, node) in rest {
                parts.push(PatternPart::Edge(edge));
                parts.push(PatternPart::Node(node));
            }
            parts
        })
}

/// Parser for edge followed by node
fn edge_then_node_parser<'src>() -> impl Parser<'src, &'src str, (EdgePart, NodePart), Extra<'src>> {
    edge_parser().then(node_parser())
}

// ============================================================================
// Node parsing
// ============================================================================

#[derive(Debug, Clone)]
struct NodePart {
    alias: Option<String>,
    label: Option<String>,
    properties: Vec<(String, String)>, // key, value pairs
}

/// Parser for node: (alias:Label {prop: 'value'})
fn node_parser<'src>() -> impl Parser<'src, &'src str, NodePart, Extra<'src>> {
    // Identifier: alphanumeric + underscore
    let ident = any()
        .filter(|c: &char| c.is_alphanumeric() || *c == '_')
        .repeated()
        .at_least(1)
        .to_slice()
        .map(|s: &str| s.to_string())
        .labelled("identifier");

    // Optional alias (before colon or by itself)
    let alias = ident.clone().or_not();

    // Optional label: :Label
    let label = just(':')
        .ignore_then(ident.clone())
        .or_not()
        .labelled("node label like :Note");

    // String literal: 'value' or "value"
    let single_quoted = just('\'')
        .ignore_then(none_of("'").repeated().to_slice().map(|s: &str| s.to_string()))
        .then_ignore(just('\''));

    let double_quoted = just('"')
        .ignore_then(none_of("\"").repeated().to_slice().map(|s: &str| s.to_string()))
        .then_ignore(just('"'));

    let string_literal = single_quoted
        .or(double_quoted)
        .labelled("string literal like 'value'");

    // Property: key: 'value'
    let property = ident
        .clone()
        .padded()
        .then_ignore(just(':'))
        .padded()
        .then(string_literal)
        .labelled("property like title: 'value'");

    // Properties block: {key: 'value', key2: 'value2'}
    let properties = just('{')
        .padded()
        .ignore_then(
            property
                .separated_by(just(',').padded())
                .allow_trailing()
                .collect::<Vec<_>>(),
        )
        .then_ignore(just('}').padded())
        .or_not()
        .map(|opt| opt.unwrap_or_default())
        .labelled("properties like {title: 'value'}");

    // Full node: (alias:Label {props})
    just('(')
        .padded()
        .ignore_then(alias)
        .then(label)
        .then(properties)
        .then_ignore(just(')').padded())
        .map(|((alias, label), properties)| NodePart {
            alias,
            label,
            properties,
        })
        .labelled("node pattern like (a:Note {title: 'X'})")
}

// ============================================================================
// Edge parsing
// ============================================================================

#[derive(Debug, Clone)]
struct EdgePart {
    alias: Option<String>,
    edge_type: Option<String>,
    direction: EdgeDirection,
}

/// Parser for edge: -[:type]-> or <-[:type]- or -[:type]- or <-[:type]->
fn edge_parser<'src>() -> impl Parser<'src, &'src str, EdgePart, Extra<'src>> {
    // Identifier for edge type
    let ident = any()
        .filter(|c: &char| c.is_alphanumeric() || *c == '_')
        .repeated()
        .at_least(1)
        .to_slice()
        .map(|s: &str| s.to_string());

    // Optional alias
    let alias = ident.clone().or_not();

    // Edge type: :type
    let edge_type = just(':')
        .padded()
        .ignore_then(ident)
        .or_not()
        .labelled("edge type like :wikilink");

    // Edge inner: [alias:type] or [:type] or []
    let edge_inner = just('[')
        .padded()
        .ignore_then(alias)
        .then(edge_type)
        .then_ignore(just(']').padded())
        .labelled("edge specification like [:wikilink]");

    // Direction variants:
    // -[...]-> = Out
    // <-[...]- = In
    // -[...]- = Undirected
    // <-[...]-> = Both

    // Outgoing: -[...]->
    let right_arrow = just('-')
        .padded()
        .ignore_then(edge_inner.clone())
        .then_ignore(just("->").padded())
        .map(|(alias, edge_type)| EdgePart {
            alias,
            edge_type,
            direction: EdgeDirection::Out,
        })
        .labelled("outgoing edge like -[:wikilink]->");

    // Incoming: <-[...]-
    let left_arrow = just("<-")
        .padded()
        .ignore_then(edge_inner.clone())
        .then_ignore(just('-').padded())
        .map(|(alias, edge_type)| EdgePart {
            alias,
            edge_type,
            direction: EdgeDirection::In,
        })
        .labelled("incoming edge like <-[:wikilink]-");

    // Bidirectional: <-[...]->
    let bidirectional = just("<-")
        .padded()
        .ignore_then(edge_inner.clone())
        .then_ignore(just("->").padded())
        .map(|(alias, edge_type)| EdgePart {
            alias,
            edge_type,
            direction: EdgeDirection::Both,
        })
        .labelled("bidirectional edge like <-[:wikilink]->");

    // Undirected: -[...]- (but NOT -[]->)
    let undirected = just('-')
        .padded()
        .ignore_then(edge_inner)
        .then_ignore(just('-').padded())
        .map(|(alias, edge_type)| EdgePart {
            alias,
            edge_type,
            direction: EdgeDirection::Undirected,
        })
        .labelled("undirected edge like -[:wikilink]-");

    // Try specific patterns first (longer match), then fallback
    choice((bidirectional, left_arrow, right_arrow, undirected))
}

// ============================================================================
// AST to IR conversion
// ============================================================================

#[derive(Debug)]
enum PatternPart {
    Node(NodePart),
    Edge(EdgePart),
}

/// Build GraphIR from parsed pattern parts
fn build_graph_ir(parts: Vec<PatternPart>) -> GraphIR {
    // Extract title from node with properties
    let mut source_title: Option<String> = None;
    let mut elements: Vec<PatternElement> = Vec::new();

    for part in parts {
        match part {
            PatternPart::Node(node) => {
                // Check if this node has a title property
                for (key, value) in &node.properties {
                    if key == "title" && source_title.is_none() {
                        source_title = Some(value.clone());
                    }
                }

                // Convert to NodePattern
                let node_pattern = NodePattern {
                    alias: node.alias,
                    label: node.label,
                    properties: node
                        .properties
                        .into_iter()
                        .map(|(k, v)| PropertyMatch {
                            key: k,
                            op: MatchOp::Eq,
                            value: Value::String(v),
                        })
                        .collect(),
                };
                elements.push(PatternElement::Node(node_pattern));
            }
            PatternPart::Edge(edge) => {
                let edge_pattern = EdgePattern {
                    alias: edge.alias,
                    edge_type: edge.edge_type,
                    direction: edge.direction,
                    quantifier: None,
                };
                elements.push(PatternElement::Edge(edge_pattern));
            }
        }
    }

    let source = source_title
        .map(QuerySource::ByTitle)
        .unwrap_or(QuerySource::All);

    GraphIR {
        source,
        pattern: GraphPattern { elements },
        projections: Vec::new(),
        filters: Vec::new(),
        post_filter: None,
    }
}

// ============================================================================
// Error formatting
// ============================================================================

/// Format chumsky errors for LLM consumption
fn format_chumsky_errors(errs: &[Rich<'_, char>], input: &str) -> String {
    errs.iter()
        .map(|e| {
            let span = e.span();
            let start = span.start;
            let line = input[..start].lines().count().max(1);
            let col = start - input[..start].rfind('\n').map_or(0, |i| i + 1);

            let found = e
                .found()
                .map_or("end of input".to_string(), |c| format!("'{}'", c));

            // Rich errors have a reason() method instead of expected()
            let reason = format!("{}", e.reason());

            format!(
                "Line {}, column {}: {} (found {})",
                line,
                col + 1,
                reason,
                found
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // can_handle tests
    // =========================================================================

    #[test]
    fn test_can_handle_match() {
        let syntax = PgqSyntax;
        assert!(syntax.can_handle("MATCH (a)-[:wikilink]->(b)"));
    }

    #[test]
    fn test_can_handle_match_lowercase() {
        let syntax = PgqSyntax;
        assert!(syntax.can_handle("match (a)-[:wikilink]->(b)"));
    }

    #[test]
    fn test_can_handle_from_graph() {
        let syntax = PgqSyntax;
        assert!(syntax.can_handle("FROM GRAPH notes MATCH (a)->(b)"));
    }

    #[test]
    fn test_cannot_handle_select() {
        let syntax = PgqSyntax;
        assert!(!syntax.can_handle("SELECT outlinks FROM 'Index'"));
    }

    #[test]
    fn test_cannot_handle_jaq() {
        let syntax = PgqSyntax;
        assert!(!syntax.can_handle(r#"outlinks("Index")"#));
    }

    #[test]
    fn test_priority_is_default() {
        let syntax = PgqSyntax;
        assert_eq!(syntax.priority(), 50);
    }

    // =========================================================================
    // Basic parsing tests
    // =========================================================================

    #[test]
    fn test_parse_simple_outlinks() {
        let syntax = PgqSyntax;
        let ir = syntax
            .parse("MATCH (a {title: 'Index'})-[:wikilink]->(b)")
            .unwrap();

        assert_eq!(ir.source, QuerySource::ByTitle("Index".to_string()));
        assert_eq!(ir.pattern.elements.len(), 3); // node, edge, node

        // First node has title property
        if let PatternElement::Node(node) = &ir.pattern.elements[0] {
            assert_eq!(node.alias, Some("a".to_string()));
            assert_eq!(node.properties.len(), 1);
            assert_eq!(node.properties[0].key, "title");
        } else {
            panic!("Expected node pattern");
        }

        // Edge is outgoing
        if let PatternElement::Edge(edge) = &ir.pattern.elements[1] {
            assert_eq!(edge.direction, EdgeDirection::Out);
            assert_eq!(edge.edge_type, Some("wikilink".to_string()));
        } else {
            panic!("Expected edge pattern");
        }

        // Second node has alias b
        if let PatternElement::Node(node) = &ir.pattern.elements[2] {
            assert_eq!(node.alias, Some("b".to_string()));
        } else {
            panic!("Expected node pattern");
        }
    }

    #[test]
    fn test_parse_simple_inlinks() {
        let syntax = PgqSyntax;
        let ir = syntax
            .parse("MATCH (a)<-[:wikilink]-(b {title: 'Index'})")
            .unwrap();

        assert_eq!(ir.source, QuerySource::ByTitle("Index".to_string()));

        // Edge is incoming
        if let PatternElement::Edge(edge) = &ir.pattern.elements[1] {
            assert_eq!(edge.direction, EdgeDirection::In);
            assert_eq!(edge.edge_type, Some("wikilink".to_string()));
        } else {
            panic!("Expected edge pattern");
        }
    }

    #[test]
    fn test_parse_undirected() {
        let syntax = PgqSyntax;
        let ir = syntax
            .parse("MATCH (a {title: 'Hub'})-[:wikilink]-(b)")
            .unwrap();

        assert_eq!(ir.source, QuerySource::ByTitle("Hub".to_string()));

        if let PatternElement::Edge(edge) = &ir.pattern.elements[1] {
            assert_eq!(edge.direction, EdgeDirection::Undirected);
        } else {
            panic!("Expected edge pattern");
        }
    }

    #[test]
    fn test_parse_bidirectional() {
        let syntax = PgqSyntax;
        let ir = syntax
            .parse("MATCH (a {title: 'Hub'})<-[:wikilink]->(b)")
            .unwrap();

        assert_eq!(ir.source, QuerySource::ByTitle("Hub".to_string()));

        if let PatternElement::Edge(edge) = &ir.pattern.elements[1] {
            assert_eq!(edge.direction, EdgeDirection::Both);
        } else {
            panic!("Expected edge pattern");
        }
    }

    // =========================================================================
    // Node parsing tests
    // =========================================================================

    #[test]
    fn test_parse_node_alias_only() {
        let syntax = PgqSyntax;
        let ir = syntax
            .parse("MATCH (source {title: 'X'})-[:link]->(target)")
            .unwrap();

        if let PatternElement::Node(node) = &ir.pattern.elements[2] {
            assert_eq!(node.alias, Some("target".to_string()));
            assert!(node.properties.is_empty());
        } else {
            panic!("Expected node pattern");
        }
    }

    #[test]
    fn test_parse_node_with_label() {
        let syntax = PgqSyntax;
        let ir = syntax
            .parse("MATCH (a:Note {title: 'X'})-[:link]->(b)")
            .unwrap();

        if let PatternElement::Node(node) = &ir.pattern.elements[0] {
            assert_eq!(node.alias, Some("a".to_string()));
            assert_eq!(node.label, Some("Note".to_string()));
        } else {
            panic!("Expected node pattern");
        }
    }

    #[test]
    fn test_parse_node_double_quotes() {
        let syntax = PgqSyntax;
        let ir = syntax
            .parse(r#"MATCH (a {title: "Index"})-[:wikilink]->(b)"#)
            .unwrap();

        assert_eq!(ir.source, QuerySource::ByTitle("Index".to_string()));
    }

    // =========================================================================
    // Edge parsing tests
    // =========================================================================

    #[test]
    fn test_parse_edge_no_type() {
        let syntax = PgqSyntax;
        let ir = syntax.parse("MATCH (a {title: 'X'})-[]->(b)").unwrap();

        if let PatternElement::Edge(edge) = &ir.pattern.elements[1] {
            assert_eq!(edge.edge_type, None);
            assert_eq!(edge.direction, EdgeDirection::Out);
        } else {
            panic!("Expected edge pattern");
        }
    }

    #[test]
    fn test_parse_edge_with_alias() {
        let syntax = PgqSyntax;
        let ir = syntax
            .parse("MATCH (a {title: 'X'})-[e:wikilink]->(b)")
            .unwrap();

        if let PatternElement::Edge(edge) = &ir.pattern.elements[1] {
            assert_eq!(edge.alias, Some("e".to_string()));
            assert_eq!(edge.edge_type, Some("wikilink".to_string()));
        } else {
            panic!("Expected edge pattern");
        }
    }

    // =========================================================================
    // Error handling tests
    // =========================================================================

    #[test]
    fn test_parse_error_unclosed_paren() {
        let syntax = PgqSyntax;
        let result = syntax.parse("MATCH (a {title: 'X'}-[:wikilink]->(b)");

        assert!(result.is_err());
        let err = result.unwrap_err();
        if let ParseError::Pgq { errors } = err {
            assert!(!errors.is_empty());
        } else {
            panic!("Expected ParseError::Pgq");
        }
    }

    #[test]
    fn test_parse_error_missing_bracket() {
        let syntax = PgqSyntax;
        let result = syntax.parse("MATCH (a {title: 'X'})->wikilink]->(b)");

        assert!(result.is_err());
    }

    // =========================================================================
    // Case insensitivity tests
    // =========================================================================

    #[test]
    fn test_parse_lowercase_match() {
        let syntax = PgqSyntax;
        let ir = syntax.parse("match (a {title: 'X'})-[:link]->(b)").unwrap();

        assert_eq!(ir.source, QuerySource::ByTitle("X".to_string()));
    }

    #[test]
    fn test_parse_mixed_case_match() {
        let syntax = PgqSyntax;
        let ir = syntax.parse("Match (a {title: 'X'})-[:link]->(b)").unwrap();

        assert_eq!(ir.source, QuerySource::ByTitle("X".to_string()));
    }

    // =========================================================================
    // Whitespace handling tests
    // =========================================================================

    #[test]
    fn test_parse_with_extra_whitespace() {
        let syntax = PgqSyntax;
        let ir = syntax
            .parse("MATCH  (  a  { title :  'X'  }  )  -  [ : wikilink ]  ->  ( b )")
            .unwrap();

        assert_eq!(ir.source, QuerySource::ByTitle("X".to_string()));
    }
}
