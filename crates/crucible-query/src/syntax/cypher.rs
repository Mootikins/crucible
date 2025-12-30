//! Cypher query syntax parser using chumsky.
//!
//! Extends the PGQ MATCH parser with:
//! - WHERE clauses with AND conditions
//! - RETURN projections with aliases
//! - Variable-length paths (*1..3, *, +)
//! - Parameter placeholders ($name)
//!
//! Priority: 55 (higher than PGQ's 50)

use crate::error::ParseError;
use crate::ir::{
    EdgeDirection, EdgePattern, Filter, GraphIR, GraphPattern, MatchOp, NodePattern,
    PatternElement, Projection, PropertyMatch, Quantifier, QuerySource,
};
use crate::syntax::common::{format_errors, ident, string_literal, Extra};
use crate::syntax::QuerySyntax;
use chumsky::prelude::*;
use chumsky::span::SimpleSpan;
use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::Value;

/// Fast prefix check for Cypher MATCH keyword
/// Note: CREATE/DELETE/MERGE are not yet supported
static CYPHER_PREFIX_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)^\s*MATCH\b").unwrap());

/// Cypher query syntax parser
pub struct CypherSyntax;

impl QuerySyntax for CypherSyntax {
    fn name(&self) -> &'static str {
        "cypher"
    }

    fn can_handle(&self, input: &str) -> bool {
        CYPHER_PREFIX_RE.is_match(input)
    }

    fn parse(&self, input: &str) -> Result<GraphIR, ParseError> {
        cypher_query_parser()
            .parse(input)
            .into_result()
            .map_err(|errs| ParseError::Cypher {
                errors: format_errors(&errs, input),
            })
    }

    fn priority(&self) -> u8 {
        55 // Higher than PGQ (50)
    }
}

// ============================================================================
// Intermediate types
// ============================================================================

#[derive(Debug, Clone)]
struct CypherNodePart {
    alias: Option<String>,
    label: Option<String>,
    properties: Vec<(String, Value)>,
}

#[derive(Debug, Clone)]
struct CypherEdgePart {
    alias: Option<String>,
    edge_type: Option<String>,
    direction: EdgeDirection,
    quantifier: Option<Quantifier>,
}

#[derive(Debug, Clone)]
enum CypherPatternPart {
    Node(CypherNodePart),
    Edge(CypherEdgePart),
}

// ============================================================================
// Main parser
// ============================================================================

/// Main Cypher query parser - currently only MATCH queries
fn cypher_query_parser<'src>() -> impl Parser<'src, &'src str, GraphIR, Extra<'src>> {
    match_query_parser().padded()
}

/// MATCH query: MATCH pattern [WHERE conditions] [RETURN projections]
fn match_query_parser<'src>() -> impl Parser<'src, &'src str, GraphIR, Extra<'src>> {
    let match_kw = kw("MATCH");

    match_kw
        .ignore_then(graph_pattern_parser())
        .then(where_clause_parser().or_not())
        .then(return_clause_parser().or_not())
        .map(|((pattern, where_clause), return_clause)| {
            build_match_ir(pattern, where_clause, return_clause)
        })
}

// ============================================================================
// Graph pattern with quantifiers
// ============================================================================

fn graph_pattern_parser<'src>() -> impl Parser<'src, &'src str, Vec<CypherPatternPart>, Extra<'src>>
{
    cypher_node_parser()
        .then(edge_then_node_parser().repeated().collect::<Vec<_>>())
        .map(|(first, rest)| {
            let mut parts = vec![CypherPatternPart::Node(first)];
            for (edge, node) in rest {
                parts.push(CypherPatternPart::Edge(edge));
                parts.push(CypherPatternPart::Node(node));
            }
            parts
        })
}

fn edge_then_node_parser<'src>(
) -> impl Parser<'src, &'src str, (CypherEdgePart, CypherNodePart), Extra<'src>> {
    cypher_edge_parser().then(cypher_node_parser())
}

// ============================================================================
// Node parsing with Cypher values
// ============================================================================

fn cypher_node_parser<'src>() -> impl Parser<'src, &'src str, CypherNodePart, Extra<'src>> {
    let alias = ident().or_not();
    let label = just(':').ignore_then(ident()).or_not();

    just('(')
        .padded()
        .ignore_then(alias)
        .then(label)
        .then(cypher_properties_block())
        .then_ignore(just(')').padded())
        .map(|((alias, label), properties)| CypherNodePart {
            alias,
            label,
            properties,
        })
        .labelled("node pattern like (a:Note {path: 'X'})")
}

fn cypher_properties_block<'src>() -> impl Parser<'src, &'src str, Vec<(String, Value)>, Extra<'src>>
{
    let property = ident()
        .padded()
        .then_ignore(just(':'))
        .padded()
        .then(value_parser())
        .labelled("property like title: 'value'");

    just('{')
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
}

// ============================================================================
// Edge parsing with quantifiers
// ============================================================================

fn cypher_edge_parser<'src>() -> impl Parser<'src, &'src str, CypherEdgePart, Extra<'src>> {
    // Edge inner: [alias:TYPE*1..3]
    let edge_inner = edge_inner_parser();

    // Bidirectional: <-[...]->
    let bidirectional = just("<-")
        .padded()
        .ignore_then(edge_inner.clone())
        .then_ignore(just("->").padded())
        .map(|(alias, edge_type, quantifier)| CypherEdgePart {
            alias,
            edge_type,
            direction: EdgeDirection::Both,
            quantifier,
        })
        .labelled("bidirectional edge like <-[:wikilink]->");

    // Incoming: <-[...]-
    let incoming = just("<-")
        .padded()
        .ignore_then(edge_inner.clone())
        .then_ignore(just('-').padded())
        .map(|(alias, edge_type, quantifier)| CypherEdgePart {
            alias,
            edge_type,
            direction: EdgeDirection::In,
            quantifier,
        })
        .labelled("incoming edge like <-[:wikilink]-");

    // Outgoing: -[...]->
    let outgoing = just('-')
        .padded()
        .ignore_then(edge_inner.clone())
        .then_ignore(just("->").padded())
        .map(|(alias, edge_type, quantifier)| CypherEdgePart {
            alias,
            edge_type,
            direction: EdgeDirection::Out,
            quantifier,
        })
        .labelled("outgoing edge like -[:wikilink]->");

    // Undirected: -[...]-
    let undirected = just('-')
        .padded()
        .ignore_then(edge_inner)
        .then_ignore(just('-').padded())
        .map(|(alias, edge_type, quantifier)| CypherEdgePart {
            alias,
            edge_type,
            direction: EdgeDirection::Undirected,
            quantifier,
        })
        .labelled("undirected edge like -[:wikilink]-");

    choice((bidirectional, incoming, outgoing, undirected))
}

/// Inner edge content: [alias:TYPE*1..3]
fn edge_inner_parser<'src>(
) -> impl Parser<'src, &'src str, (Option<String>, Option<String>, Option<Quantifier>), Extra<'src>>
       + Clone {
    let alias = ident().or_not();
    let edge_type = just(':')
        .padded()
        .ignore_then(ident())
        .or_not()
        .labelled("edge type like :LINKS_TO");
    let quantifier = quantifier_parser().or_not();

    just('[')
        .padded()
        .ignore_then(alias)
        .then(edge_type)
        .then(quantifier)
        .then_ignore(just(']').padded())
        .map(|((alias, edge_type), quantifier)| (alias, edge_type, quantifier))
        .labelled("edge specification like [:LINKS_TO*1..3]")
}

// ============================================================================
// Path quantifiers: *, +, *1..3, *..5, *2..
// ============================================================================

fn quantifier_parser<'src>() -> impl Parser<'src, &'src str, Quantifier, Extra<'src>> + Clone {
    let number = any()
        .filter(|c: &char| c.is_ascii_digit())
        .repeated()
        .at_least(1)
        .to_slice()
        .try_map(|s: &str, span| {
            s.parse::<usize>()
                .map_err(|_| Rich::custom(span, "integer overflow in quantifier"))
        });

    // *1..3 (range with both bounds)
    let range_both = just('*')
        .ignore_then(number)
        .then_ignore(just(".."))
        .then(number)
        .map(|(min, max)| Quantifier::Range {
            min,
            max: Some(max),
        });

    // *..3 (range with max only, min=0)
    let range_max = just("*..")
        .ignore_then(number)
        .map(|max| Quantifier::Range {
            min: 0,
            max: Some(max),
        });

    // *2.. (range with min only, no max)
    let range_min = just('*')
        .ignore_then(number)
        .then_ignore(just(".."))
        .map(|min| Quantifier::Range { min, max: None });

    // * (zero or more)
    let star = just('*').to(Quantifier::ZeroOrMore);

    // + (one or more)
    let plus = just('+').to(Quantifier::OneOrMore);

    choice((range_both, range_max, range_min, star, plus)).padded()
}

// ============================================================================
// WHERE clause: WHERE condition [AND condition]*
// ============================================================================

fn where_clause_parser<'src>() -> impl Parser<'src, &'src str, Vec<Filter>, Extra<'src>> {
    kw("WHERE").ignore_then(
        condition_parser()
            .separated_by(kw("AND"))
            .at_least(1)
            .collect(),
    )
}

fn condition_parser<'src>() -> impl Parser<'src, &'src str, Filter, Extra<'src>> {
    // alias.property (e.g., n.folder)
    let field = ident()
        .then_ignore(just('.'))
        .then(ident())
        .map(|(alias, prop)| format!("{}.{}", alias, prop));

    // Comparison operators
    let op = choice((
        just("!=").or(just("<>")).to(MatchOp::Ne),
        just("=").to(MatchOp::Eq),
        kw("STARTS").then(kw("WITH")).to(MatchOp::StartsWith),
        kw("ENDS").then(kw("WITH")).to(MatchOp::EndsWith),
        kw("CONTAINS").to(MatchOp::Contains),
    ))
    .padded();

    field
        .padded()
        .then(op)
        .then(value_parser())
        .map(|((field, op), value)| Filter { field, op, value })
}

// ============================================================================
// RETURN clause: RETURN projection [, projection]*
// ============================================================================

fn return_clause_parser<'src>() -> impl Parser<'src, &'src str, Vec<Projection>, Extra<'src>> {
    kw("RETURN").ignore_then(
        projection_parser()
            .separated_by(just(',').padded())
            .at_least(1)
            .collect(),
    )
}

fn projection_parser<'src>() -> impl Parser<'src, &'src str, Projection, Extra<'src>> {
    // alias.property OR alias
    let field =
        ident()
            .then(just('.').ignore_then(ident()).or_not())
            .map(|(alias, prop)| match prop {
                Some(p) => format!("{}.{}", alias, p),
                None => alias,
            });

    // Optional AS name
    let as_alias = kw("AS").ignore_then(ident()).or_not();

    field
        .padded()
        .then(as_alias)
        .map(|(field, alias)| Projection { field, alias })
}

// ============================================================================
// Value parsing
// ============================================================================

fn value_parser<'src>() -> impl Parser<'src, &'src str, Value, Extra<'src>> + Clone {
    // String literal
    let string = string_literal().map(Value::String);

    // Number (integer or float) - with proper error handling
    let number = just('-')
        .or_not()
        .then(
            any()
                .filter(|c: &char| c.is_ascii_digit())
                .repeated()
                .at_least(1),
        )
        .then(
            just('.')
                .then(any().filter(|c: &char| c.is_ascii_digit()).repeated())
                .or_not(),
        )
        .to_slice()
        .try_map(|s: &str, span: SimpleSpan| {
            if s.contains('.') {
                let f: f64 = s
                    .parse()
                    .map_err(|_| Rich::custom(span, "invalid float literal"))?;
                serde_json::Number::from_f64(f)
                    .map(Value::Number)
                    .ok_or_else(|| Rich::custom(span, "invalid float (NaN or Infinity)"))
            } else {
                s.parse::<i64>()
                    .map(|n| Value::Number(n.into()))
                    .map_err(|_| Rich::custom(span, "integer overflow"))
            }
        });

    // Boolean
    let boolean = choice((
        kw("true").to(Value::Bool(true)),
        kw("false").to(Value::Bool(false)),
    ));

    // Null
    let null = kw("null").to(Value::Null);

    // Parameter: $name (stored as "$name" string for later substitution)
    let param = just('$')
        .ignore_then(ident())
        .map(|name| Value::String(format!("${}", name)));

    choice((param, string, number, boolean, null)).padded()
}

// ============================================================================
// Helpers
// ============================================================================

/// Case-insensitive keyword matcher
fn kw<'src>(keyword: &'static str) -> impl Parser<'src, &'src str, (), Extra<'src>> + Clone {
    // Match the keyword case-insensitively by checking each character
    any()
        .filter(|c: &char| c.is_alphabetic())
        .repeated()
        .at_least(1)
        .to_slice()
        .try_map(move |s: &str, span| {
            if s.eq_ignore_ascii_case(keyword) {
                Ok(())
            } else {
                Err(Rich::custom(
                    span,
                    format!("expected keyword '{}'", keyword),
                ))
            }
        })
        .padded()
}

// ============================================================================
// IR construction
// ============================================================================

fn build_match_ir(
    parts: Vec<CypherPatternPart>,
    where_clause: Option<Vec<Filter>>,
    return_clause: Option<Vec<Projection>>,
) -> GraphIR {
    let mut elements = Vec::new();
    let mut source = QuerySource::All;

    for part in parts {
        match part {
            CypherPatternPart::Node(node) => {
                // Extract source from first node with path/title property
                for (key, value) in &node.properties {
                    if source == QuerySource::All {
                        if key == "path" {
                            if let Value::String(s) = value {
                                // Skip parameter placeholders
                                if !s.starts_with('$') {
                                    source = QuerySource::ByPath(s.clone());
                                }
                            }
                        } else if key == "title" {
                            if let Value::String(s) = value {
                                if !s.starts_with('$') {
                                    source = QuerySource::ByTitle(s.clone());
                                }
                            }
                        }
                    }
                }

                elements.push(PatternElement::Node(NodePattern {
                    alias: node.alias,
                    label: node.label,
                    properties: node
                        .properties
                        .into_iter()
                        .map(|(k, v)| PropertyMatch {
                            key: k,
                            op: MatchOp::Eq,
                            value: v,
                        })
                        .collect(),
                }));
            }
            CypherPatternPart::Edge(edge) => {
                elements.push(PatternElement::Edge(EdgePattern {
                    alias: edge.alias,
                    edge_type: edge.edge_type,
                    direction: edge.direction,
                    quantifier: edge.quantifier,
                }));
            }
        }
    }

    GraphIR {
        source,
        pattern: GraphPattern { elements },
        projections: return_clause.unwrap_or_default(),
        filters: where_clause.unwrap_or_default(),
        post_filter: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // can_handle tests
    // =========================================================================

    #[test]
    fn test_can_handle_match() {
        let syntax = CypherSyntax;
        assert!(syntax.can_handle("MATCH (n) RETURN n"));
    }

    #[test]
    fn test_can_handle_match_lowercase() {
        let syntax = CypherSyntax;
        assert!(syntax.can_handle("match (n) return n"));
    }

    #[test]
    fn test_cannot_handle_select() {
        let syntax = CypherSyntax;
        assert!(!syntax.can_handle("SELECT * FROM notes"));
    }

    #[test]
    fn test_priority_is_higher_than_pgq() {
        let syntax = CypherSyntax;
        assert_eq!(syntax.priority(), 55);
    }

    // =========================================================================
    // Basic MATCH tests
    // =========================================================================

    #[test]
    fn test_parse_simple_match() {
        let syntax = CypherSyntax;
        let ir = syntax.parse("MATCH (n:Note) RETURN n").unwrap();

        assert_eq!(ir.pattern.elements.len(), 1);
        assert_eq!(ir.projections.len(), 1);
        assert_eq!(ir.projections[0].field, "n");
    }

    #[test]
    fn test_parse_match_with_path() {
        let syntax = CypherSyntax;
        let ir = syntax
            .parse("MATCH (n:Note {path: 'index.md'}) RETURN n")
            .unwrap();

        assert_eq!(ir.source, QuerySource::ByPath("index.md".to_string()));
    }

    #[test]
    fn test_parse_match_with_title() {
        let syntax = CypherSyntax;
        let ir = syntax
            .parse("MATCH (n:Note {title: 'Index'}) RETURN n")
            .unwrap();

        assert_eq!(ir.source, QuerySource::ByTitle("Index".to_string()));
    }

    // =========================================================================
    // Edge pattern tests
    // =========================================================================

    #[test]
    fn test_parse_outgoing_edge() {
        let syntax = CypherSyntax;
        let ir = syntax
            .parse("MATCH (a:Note)-[:LINKS_TO]->(b) RETURN b")
            .unwrap();

        assert_eq!(ir.pattern.elements.len(), 3);
        if let PatternElement::Edge(edge) = &ir.pattern.elements[1] {
            assert_eq!(edge.direction, EdgeDirection::Out);
            assert_eq!(edge.edge_type, Some("LINKS_TO".to_string()));
        } else {
            panic!("Expected edge");
        }
    }

    #[test]
    fn test_parse_incoming_edge() {
        let syntax = CypherSyntax;
        let ir = syntax
            .parse("MATCH (a:Note)<-[:LINKS_TO]-(b) RETURN b")
            .unwrap();

        if let PatternElement::Edge(edge) = &ir.pattern.elements[1] {
            assert_eq!(edge.direction, EdgeDirection::In);
        } else {
            panic!("Expected edge");
        }
    }

    #[test]
    fn test_parse_bidirectional_edge() {
        let syntax = CypherSyntax;
        let ir = syntax
            .parse("MATCH (a:Note)<-[:LINKS_TO]->(b) RETURN b")
            .unwrap();

        if let PatternElement::Edge(edge) = &ir.pattern.elements[1] {
            assert_eq!(edge.direction, EdgeDirection::Both);
        } else {
            panic!("Expected edge");
        }
    }

    #[test]
    fn test_parse_undirected_edge() {
        let syntax = CypherSyntax;
        let ir = syntax
            .parse("MATCH (a:Note)-[:LINKS_TO]-(b) RETURN b")
            .unwrap();

        if let PatternElement::Edge(edge) = &ir.pattern.elements[1] {
            assert_eq!(edge.direction, EdgeDirection::Undirected);
        } else {
            panic!("Expected edge");
        }
    }

    // =========================================================================
    // Path quantifier tests
    // =========================================================================

    #[test]
    fn test_parse_quantifier_range() {
        let syntax = CypherSyntax;
        let ir = syntax
            .parse("MATCH (a)-[:LINKS_TO*1..3]->(b) RETURN b")
            .unwrap();

        if let PatternElement::Edge(edge) = &ir.pattern.elements[1] {
            assert_eq!(
                edge.quantifier,
                Some(Quantifier::Range {
                    min: 1,
                    max: Some(3)
                })
            );
        } else {
            panic!("Expected edge");
        }
    }

    #[test]
    fn test_parse_quantifier_star() {
        let syntax = CypherSyntax;
        let ir = syntax
            .parse("MATCH (a)-[:LINKS_TO*]->(b) RETURN b")
            .unwrap();

        if let PatternElement::Edge(edge) = &ir.pattern.elements[1] {
            assert_eq!(edge.quantifier, Some(Quantifier::ZeroOrMore));
        } else {
            panic!("Expected edge");
        }
    }

    #[test]
    fn test_parse_quantifier_plus() {
        let syntax = CypherSyntax;
        let ir = syntax
            .parse("MATCH (a)-[:LINKS_TO+]->(b) RETURN b")
            .unwrap();

        if let PatternElement::Edge(edge) = &ir.pattern.elements[1] {
            assert_eq!(edge.quantifier, Some(Quantifier::OneOrMore));
        } else {
            panic!("Expected edge");
        }
    }

    #[test]
    fn test_parse_quantifier_min_only() {
        let syntax = CypherSyntax;
        let ir = syntax
            .parse("MATCH (a)-[:LINKS_TO*2..]->(b) RETURN b")
            .unwrap();

        if let PatternElement::Edge(edge) = &ir.pattern.elements[1] {
            assert_eq!(
                edge.quantifier,
                Some(Quantifier::Range { min: 2, max: None })
            );
        } else {
            panic!("Expected edge");
        }
    }

    #[test]
    fn test_parse_quantifier_max_only() {
        let syntax = CypherSyntax;
        let ir = syntax
            .parse("MATCH (a)-[:LINKS_TO*..5]->(b) RETURN b")
            .unwrap();

        if let PatternElement::Edge(edge) = &ir.pattern.elements[1] {
            assert_eq!(
                edge.quantifier,
                Some(Quantifier::Range {
                    min: 0,
                    max: Some(5)
                })
            );
        } else {
            panic!("Expected edge");
        }
    }

    // =========================================================================
    // WHERE clause tests
    // =========================================================================

    #[test]
    fn test_parse_where_equals() {
        let syntax = CypherSyntax;
        let ir = syntax
            .parse("MATCH (n:Note) WHERE n.folder = 'Projects' RETURN n")
            .unwrap();

        assert_eq!(ir.filters.len(), 1);
        assert_eq!(ir.filters[0].field, "n.folder");
        assert_eq!(ir.filters[0].op, MatchOp::Eq);
        assert_eq!(ir.filters[0].value, Value::String("Projects".to_string()));
    }

    #[test]
    fn test_parse_where_not_equals() {
        let syntax = CypherSyntax;
        let ir = syntax
            .parse("MATCH (n:Note) WHERE n.status != 'draft' RETURN n")
            .unwrap();

        assert_eq!(ir.filters.len(), 1);
        assert_eq!(ir.filters[0].op, MatchOp::Ne);
    }

    #[test]
    fn test_parse_where_contains() {
        let syntax = CypherSyntax;
        let ir = syntax
            .parse("MATCH (n:Note) WHERE n.title CONTAINS 'API' RETURN n")
            .unwrap();

        assert_eq!(ir.filters.len(), 1);
        assert_eq!(ir.filters[0].op, MatchOp::Contains);
    }

    #[test]
    fn test_parse_where_starts_with() {
        let syntax = CypherSyntax;
        let ir = syntax
            .parse("MATCH (n:Note) WHERE n.path STARTS WITH 'docs/' RETURN n")
            .unwrap();

        assert_eq!(ir.filters.len(), 1);
        assert_eq!(ir.filters[0].op, MatchOp::StartsWith);
    }

    #[test]
    fn test_parse_where_multiple_conditions() {
        let syntax = CypherSyntax;
        let ir = syntax
            .parse("MATCH (n:Note) WHERE n.folder = 'Projects' AND n.status = 'active' RETURN n")
            .unwrap();

        assert_eq!(ir.filters.len(), 2);
    }

    // =========================================================================
    // RETURN clause tests
    // =========================================================================

    #[test]
    fn test_parse_return_alias() {
        let syntax = CypherSyntax;
        let ir = syntax
            .parse("MATCH (n:Note) RETURN n.title AS name")
            .unwrap();

        assert_eq!(ir.projections.len(), 1);
        assert_eq!(ir.projections[0].field, "n.title");
        assert_eq!(ir.projections[0].alias, Some("name".to_string()));
    }

    #[test]
    fn test_parse_return_multiple() {
        let syntax = CypherSyntax;
        let ir = syntax
            .parse("MATCH (n:Note) RETURN n.path, n.title")
            .unwrap();

        assert_eq!(ir.projections.len(), 2);
        assert_eq!(ir.projections[0].field, "n.path");
        assert_eq!(ir.projections[1].field, "n.title");
    }

    #[test]
    fn test_parse_return_node_only() {
        let syntax = CypherSyntax;
        let ir = syntax.parse("MATCH (n:Note) RETURN n").unwrap();

        assert_eq!(ir.projections.len(), 1);
        assert_eq!(ir.projections[0].field, "n");
        assert_eq!(ir.projections[0].alias, None);
    }

    // =========================================================================
    // Parameter tests
    // =========================================================================

    #[test]
    fn test_parse_parameter_in_property() {
        let syntax = CypherSyntax;
        let ir = syntax
            .parse("MATCH (n:Note {path: $path}) RETURN n")
            .unwrap();

        // Source should still be All since parameter isn't resolved
        assert_eq!(ir.source, QuerySource::All);

        if let PatternElement::Node(node) = &ir.pattern.elements[0] {
            assert_eq!(node.properties.len(), 1);
            assert_eq!(node.properties[0].value, Value::String("$path".to_string()));
        } else {
            panic!("Expected node");
        }
    }

    #[test]
    fn test_parse_parameter_in_where() {
        let syntax = CypherSyntax;
        let ir = syntax
            .parse("MATCH (n:Note) WHERE n.folder = $folder RETURN n")
            .unwrap();

        assert_eq!(ir.filters.len(), 1);
        assert_eq!(ir.filters[0].value, Value::String("$folder".to_string()));
    }

    // =========================================================================
    // Value type tests
    // =========================================================================

    #[test]
    fn test_parse_integer_value() {
        let syntax = CypherSyntax;
        let ir = syntax
            .parse("MATCH (n:Note) WHERE n.priority = 5 RETURN n")
            .unwrap();

        assert_eq!(ir.filters[0].value, Value::Number(5.into()));
    }

    #[test]
    fn test_parse_boolean_value() {
        let syntax = CypherSyntax;
        let ir = syntax
            .parse("MATCH (n:Note) WHERE n.archived = false RETURN n")
            .unwrap();

        assert_eq!(ir.filters[0].value, Value::Bool(false));
    }

    #[test]
    fn test_parse_null_value() {
        let syntax = CypherSyntax;
        let ir = syntax
            .parse("MATCH (n:Note) WHERE n.parent = null RETURN n")
            .unwrap();

        assert_eq!(ir.filters[0].value, Value::Null);
    }

    // =========================================================================
    // Error handling tests
    // =========================================================================

    #[test]
    fn test_parse_error_missing_return() {
        let syntax = CypherSyntax;
        // This should parse - RETURN is optional
        let ir = syntax.parse("MATCH (n:Note)").unwrap();
        assert!(ir.projections.is_empty());
    }

    #[test]
    fn test_parse_error_invalid_pattern() {
        let syntax = CypherSyntax;
        let result = syntax.parse("MATCH n RETURN n");
        // Should fail because node needs parentheses
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_error_integer_overflow() {
        let syntax = CypherSyntax;
        // i64::MAX + 1 should overflow
        let result = syntax.parse("MATCH (n:Note) WHERE n.count = 9223372036854775808 RETURN n");
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(e.to_string().contains("integer overflow"));
        }
    }

    #[test]
    fn test_parse_negative_integer() {
        let syntax = CypherSyntax;
        let ir = syntax
            .parse("MATCH (n:Note) WHERE n.priority = -5 RETURN n")
            .unwrap();
        assert_eq!(ir.filters[0].value, Value::Number((-5i64).into()));
    }

    #[test]
    fn test_parse_float_value() {
        let syntax = CypherSyntax;
        let ir = syntax
            .parse("MATCH (n:Note) WHERE n.score = 2.5 RETURN n")
            .unwrap();
        if let Value::Number(n) = &ir.filters[0].value {
            assert!((n.as_f64().unwrap() - 2.5).abs() < 0.001);
        } else {
            panic!("Expected number value");
        }
    }

    // =========================================================================
    // Case insensitivity tests
    // =========================================================================

    #[test]
    fn test_parse_lowercase_keywords() {
        let syntax = CypherSyntax;
        let ir = syntax
            .parse("match (n:Note) where n.folder = 'X' return n")
            .unwrap();

        assert!(!ir.filters.is_empty());
        assert!(!ir.projections.is_empty());
    }

    #[test]
    fn test_parse_mixed_case() {
        let syntax = CypherSyntax;
        let ir = syntax
            .parse("Match (n:Note) Where n.folder = 'X' Return n")
            .unwrap();

        assert!(!ir.filters.is_empty());
        assert!(!ir.projections.is_empty());
    }
}
