//! Shared parser primitives for query syntaxes.
//!
//! This module contains reusable chumsky parsers for graph pattern syntax
//! that can be shared between PGQ, Cypher, and future syntaxes.

use crate::ir::EdgeDirection;
use chumsky::extra;
use chumsky::prelude::*;

/// Extra type for parsers - uses Rich errors for better messages
pub type Extra<'src> = extra::Err<Rich<'src, char>>;

// ============================================================================
// Intermediate types (AST before conversion to IR)
// ============================================================================

/// Parsed node from pattern (before IR conversion)
#[derive(Debug, Clone)]
pub struct NodePart {
    pub alias: Option<String>,
    pub label: Option<String>,
    pub properties: Vec<(String, String)>, // key, value pairs
}

/// Parsed edge from pattern (before IR conversion)
#[derive(Debug, Clone)]
pub struct EdgePart {
    pub alias: Option<String>,
    pub edge_type: Option<String>,
    pub direction: EdgeDirection,
    pub quantifier: Option<ParsedQuantifier>,
}

/// Quantifier parsed from syntax (before IR conversion)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParsedQuantifier {
    /// Zero or more: *
    ZeroOrMore,
    /// One or more: +
    OneOrMore,
    /// Exact count: *3 or {3}
    Exactly(usize),
    /// Range: *1..3 or {1,3}
    Range { min: usize, max: Option<usize> },
}

/// A part of the pattern (node or edge)
#[derive(Debug, Clone)]
pub enum PatternPart {
    Node(NodePart),
    Edge(EdgePart),
}

// ============================================================================
// Primitive parsers
// ============================================================================

/// Parser for identifiers: alphanumeric + underscore
pub fn ident<'src>() -> impl Parser<'src, &'src str, String, Extra<'src>> + Clone {
    any()
        .filter(|c: &char| c.is_alphanumeric() || *c == '_')
        .repeated()
        .at_least(1)
        .to_slice()
        .map(|s: &str| s.to_string())
        .labelled("identifier")
}

/// Parser for single-quoted string literals: 'value'
pub fn single_quoted_string<'src>() -> impl Parser<'src, &'src str, String, Extra<'src>> + Clone {
    just('\'')
        .ignore_then(
            none_of("'")
                .repeated()
                .to_slice()
                .map(|s: &str| s.to_string()),
        )
        .then_ignore(just('\''))
        .labelled("single-quoted string")
}

/// Parser for double-quoted string literals: "value"
pub fn double_quoted_string<'src>() -> impl Parser<'src, &'src str, String, Extra<'src>> + Clone {
    just('"')
        .ignore_then(
            none_of("\"")
                .repeated()
                .to_slice()
                .map(|s: &str| s.to_string()),
        )
        .then_ignore(just('"'))
        .labelled("double-quoted string")
}

/// Parser for string literals (single or double quoted)
pub fn string_literal<'src>() -> impl Parser<'src, &'src str, String, Extra<'src>> + Clone {
    single_quoted_string()
        .or(double_quoted_string())
        .labelled("string literal")
}

/// Parser for integer literals
pub fn integer<'src>() -> impl Parser<'src, &'src str, usize, Extra<'src>> + Clone {
    any()
        .filter(|c: &char| c.is_ascii_digit())
        .repeated()
        .at_least(1)
        .to_slice()
        .try_map(|s: &str, span| {
            s.parse::<usize>()
                .map_err(|_| Rich::custom(span, "integer overflow"))
        })
        .labelled("integer")
}

/// Case-insensitive keyword parser
pub fn kw<'src>(keyword: &'static str) -> impl Parser<'src, &'src str, (), Extra<'src>> + Clone {
    text::keyword::<&str, _, Extra<'src>>(keyword).ignored()
}

// ============================================================================
// Property parsers
// ============================================================================

/// Parser for a single property: key: 'value'
pub fn property<'src>() -> impl Parser<'src, &'src str, (String, String), Extra<'src>> + Clone {
    ident()
        .padded()
        .then_ignore(just(':'))
        .padded()
        .then(string_literal())
        .labelled("property like title: 'value'")
}

/// Parser for properties block: {key: 'value', key2: 'value2'}
pub fn properties_block<'src>(
) -> impl Parser<'src, &'src str, Vec<(String, String)>, Extra<'src>> + Clone {
    just('{')
        .padded()
        .ignore_then(
            property()
                .separated_by(just(',').padded())
                .allow_trailing()
                .collect::<Vec<_>>(),
        )
        .then_ignore(just('}').padded())
        .or_not()
        .map(|opt| opt.unwrap_or_default())
        .labelled("properties like {title: 'value'}")
}

// ============================================================================
// Node parsing
// ============================================================================

/// Parser for node pattern: (alias:Label {prop: 'value'})
pub fn node_parser<'src>() -> impl Parser<'src, &'src str, NodePart, Extra<'src>> + Clone {
    // Optional alias (before colon or by itself)
    let alias = ident().or_not();

    // Optional label: :Label
    let label = just(':')
        .ignore_then(ident())
        .or_not()
        .labelled("node label like :Note");

    // Full node: (alias:Label {props})
    just('(')
        .padded()
        .ignore_then(alias)
        .then(label)
        .then(properties_block())
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

/// Parser for edge inner content: [alias:type] or [:type] or []
fn edge_inner<'src>() -> impl Parser<'src, &'src str, (Option<String>, Option<String>), Extra<'src>>
{
    // Optional alias
    let alias = ident().or_not();

    // Edge type: :type
    let edge_type = just(':')
        .padded()
        .ignore_then(ident())
        .or_not()
        .labelled("edge type like :wikilink");

    just('[')
        .padded()
        .ignore_then(alias)
        .then(edge_type)
        .then_ignore(just(']').padded())
        .labelled("edge specification like [:wikilink]")
}

/// Parser for edge pattern: -[:type]-> or <-[:type]- or -[:type]- or <-[:type]->
pub fn edge_parser<'src>() -> impl Parser<'src, &'src str, EdgePart, Extra<'src>> {
    // Outgoing: -[...]->
    let right_arrow = just('-')
        .padded()
        .ignore_then(edge_inner())
        .then_ignore(just("->").padded())
        .map(|(alias, edge_type)| EdgePart {
            alias,
            edge_type,
            direction: EdgeDirection::Out,
            quantifier: None,
        })
        .labelled("outgoing edge like -[:wikilink]->");

    // Incoming: <-[...]-
    let left_arrow = just("<-")
        .padded()
        .ignore_then(edge_inner())
        .then_ignore(just('-').padded())
        .map(|(alias, edge_type)| EdgePart {
            alias,
            edge_type,
            direction: EdgeDirection::In,
            quantifier: None,
        })
        .labelled("incoming edge like <-[:wikilink]-");

    // Bidirectional: <-[...]->
    let bidirectional = just("<-")
        .padded()
        .ignore_then(edge_inner())
        .then_ignore(just("->").padded())
        .map(|(alias, edge_type)| EdgePart {
            alias,
            edge_type,
            direction: EdgeDirection::Both,
            quantifier: None,
        })
        .labelled("bidirectional edge like <-[:wikilink]->");

    // Undirected: -[...]- (but NOT -[]->)
    let undirected = just('-')
        .padded()
        .ignore_then(edge_inner())
        .then_ignore(just('-').padded())
        .map(|(alias, edge_type)| EdgePart {
            alias,
            edge_type,
            direction: EdgeDirection::Undirected,
            quantifier: None,
        })
        .labelled("undirected edge like -[:wikilink]-");

    // Try specific patterns first (longer match), then fallback
    choice((bidirectional, left_arrow, right_arrow, undirected))
}

// ============================================================================
// Graph pattern parsing
// ============================================================================

/// Parser for edge followed by node
fn edge_then_node<'src>() -> impl Parser<'src, &'src str, (EdgePart, NodePart), Extra<'src>> {
    edge_parser().then(node_parser())
}

/// Parser for graph pattern: (node)-[edge]->(node)...
pub fn graph_pattern<'src>() -> impl Parser<'src, &'src str, Vec<PatternPart>, Extra<'src>> {
    // A pattern is: node (edge node)*
    node_parser()
        .then(edge_then_node().repeated().collect::<Vec<_>>())
        .map(|(first, rest)| {
            let mut parts = vec![PatternPart::Node(first)];
            for (edge, node) in rest {
                parts.push(PatternPart::Edge(edge));
                parts.push(PatternPart::Node(node));
            }
            parts
        })
}

// ============================================================================
// Error formatting
// ============================================================================

/// Format chumsky errors for human/LLM consumption
pub fn format_errors(errs: &[Rich<'_, char>], input: &str) -> String {
    errs.iter()
        .map(|e| {
            let span = e.span();
            let start = span.start;
            let line = input[..start].lines().count().max(1);
            let col = start - input[..start].rfind('\n').map_or(0, |i| i + 1);

            let found = e
                .found()
                .map_or("end of input".to_string(), |c| format!("'{}'", c));

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
    // Primitive parser tests
    // =========================================================================

    #[test]
    fn test_ident_simple() {
        let result = ident().parse("foo").into_result();
        assert_eq!(result.unwrap(), "foo");
    }

    #[test]
    fn test_ident_with_underscore() {
        let result = ident().parse("foo_bar").into_result();
        assert_eq!(result.unwrap(), "foo_bar");
    }

    #[test]
    fn test_ident_with_number() {
        let result = ident().parse("foo123").into_result();
        assert_eq!(result.unwrap(), "foo123");
    }

    #[test]
    fn test_string_single_quoted() {
        let result = string_literal().parse("'hello'").into_result();
        assert_eq!(result.unwrap(), "hello");
    }

    #[test]
    fn test_string_double_quoted() {
        let result = string_literal().parse("\"hello\"").into_result();
        assert_eq!(result.unwrap(), "hello");
    }

    #[test]
    fn test_integer() {
        let result = integer().parse("42").into_result();
        assert_eq!(result.unwrap(), 42);
    }

    // =========================================================================
    // Property parser tests
    // =========================================================================

    #[test]
    fn test_property_simple() {
        let result = property().parse("title: 'Index'").into_result();
        let (key, value) = result.unwrap();
        assert_eq!(key, "title");
        assert_eq!(value, "Index");
    }

    #[test]
    fn test_properties_block_single() {
        let result = properties_block().parse("{title: 'Index'}").into_result();
        let props = result.unwrap();
        assert_eq!(props.len(), 1);
        assert_eq!(props[0], ("title".to_string(), "Index".to_string()));
    }

    #[test]
    fn test_properties_block_multiple() {
        let result = properties_block()
            .parse("{title: 'Index', path: 'index.md'}")
            .into_result();
        let props = result.unwrap();
        assert_eq!(props.len(), 2);
    }

    #[test]
    fn test_properties_block_empty() {
        // No properties block returns empty vec
        let result = properties_block().parse("").into_result();
        let props = result.unwrap();
        assert!(props.is_empty());
    }

    // =========================================================================
    // Node parser tests
    // =========================================================================

    #[test]
    fn test_node_alias_only() {
        let result = node_parser().parse("(a)").into_result();
        let node = result.unwrap();
        assert_eq!(node.alias, Some("a".to_string()));
        assert_eq!(node.label, None);
        assert!(node.properties.is_empty());
    }

    #[test]
    fn test_node_with_label() {
        let result = node_parser().parse("(a:Note)").into_result();
        let node = result.unwrap();
        assert_eq!(node.alias, Some("a".to_string()));
        assert_eq!(node.label, Some("Note".to_string()));
    }

    #[test]
    fn test_node_with_properties() {
        let result = node_parser().parse("(a {title: 'Index'})").into_result();
        let node = result.unwrap();
        assert_eq!(node.alias, Some("a".to_string()));
        assert_eq!(node.properties.len(), 1);
        assert_eq!(node.properties[0].0, "title");
    }

    #[test]
    fn test_node_full() {
        let result = node_parser()
            .parse("(source:Note {title: 'Index'})")
            .into_result();
        let node = result.unwrap();
        assert_eq!(node.alias, Some("source".to_string()));
        assert_eq!(node.label, Some("Note".to_string()));
        assert_eq!(node.properties.len(), 1);
    }

    // =========================================================================
    // Edge parser tests
    // =========================================================================

    #[test]
    fn test_edge_outgoing() {
        let result = edge_parser().parse("-[:wikilink]->").into_result();
        let edge = result.unwrap();
        assert_eq!(edge.direction, EdgeDirection::Out);
        assert_eq!(edge.edge_type, Some("wikilink".to_string()));
    }

    #[test]
    fn test_edge_incoming() {
        let result = edge_parser().parse("<-[:wikilink]-").into_result();
        let edge = result.unwrap();
        assert_eq!(edge.direction, EdgeDirection::In);
        assert_eq!(edge.edge_type, Some("wikilink".to_string()));
    }

    #[test]
    fn test_edge_bidirectional() {
        let result = edge_parser().parse("<-[:wikilink]->").into_result();
        let edge = result.unwrap();
        assert_eq!(edge.direction, EdgeDirection::Both);
    }

    #[test]
    fn test_edge_undirected() {
        let result = edge_parser().parse("-[:wikilink]-").into_result();
        let edge = result.unwrap();
        assert_eq!(edge.direction, EdgeDirection::Undirected);
    }

    #[test]
    fn test_edge_with_alias() {
        let result = edge_parser().parse("-[e:wikilink]->").into_result();
        let edge = result.unwrap();
        assert_eq!(edge.alias, Some("e".to_string()));
        assert_eq!(edge.edge_type, Some("wikilink".to_string()));
    }

    #[test]
    fn test_edge_no_type() {
        let result = edge_parser().parse("-[]->").into_result();
        let edge = result.unwrap();
        assert_eq!(edge.edge_type, None);
    }

    // =========================================================================
    // Graph pattern tests
    // =========================================================================

    #[test]
    fn test_graph_pattern_simple() {
        let result = graph_pattern().parse("(a)-[:wikilink]->(b)").into_result();
        let parts = result.unwrap();
        assert_eq!(parts.len(), 3); // node, edge, node
    }

    #[test]
    fn test_graph_pattern_chain() {
        let result = graph_pattern()
            .parse("(a)-[:wikilink]->(b)-[:embed]->(c)")
            .into_result();
        let parts = result.unwrap();
        assert_eq!(parts.len(), 5); // node, edge, node, edge, node
    }
}
