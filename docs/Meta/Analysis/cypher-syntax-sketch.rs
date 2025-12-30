//! Cypher syntax parser for crucible-query.
//!
//! Extends the existing PGQ MATCH parser with:
//! - WHERE clauses
//! - RETURN projections
//! - Variable-length paths (*1..3)
//! - CREATE/DELETE mutations
//!
//! This is a SKETCH - not compilable, but shows the structure needed.

use crate::error::ParseError;
use crate::ir::{
    EdgeDirection, EdgePattern, Filter, GraphIR, GraphPattern, MatchOp, NodePattern,
    PatternElement, Projection, PropertyMatch, Quantifier, QuerySource,
};
use crate::syntax::QuerySyntax;
use chumsky::prelude::*;
use chumsky::extra;

type Extra<'src> = extra::Err<Rich<'src, char>>;

// ============================================================================
// Public syntax struct
// ============================================================================

pub struct CypherSyntax;

impl QuerySyntax for CypherSyntax {
    fn name(&self) -> &'static str {
        "cypher"
    }

    fn can_handle(&self, input: &str) -> bool {
        let upper = input.trim().to_uppercase();
        upper.starts_with("MATCH")
            || upper.starts_with("CREATE")
            || upper.starts_with("DELETE")
            || upper.starts_with("MERGE")
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
        55 // Higher than PGQ (50) since it's more complete
    }
}

// ============================================================================
// Main parser - dispatches to MATCH, CREATE, DELETE
// ============================================================================

fn cypher_query_parser<'src>() -> impl Parser<'src, &'src str, GraphIR, Extra<'src>> {
    choice((
        match_query_parser(),
        create_query_parser(),
        delete_query_parser(),
    ))
    .padded()
}

// ============================================================================
// MATCH query: MATCH pattern [WHERE conditions] [RETURN projections]
// ============================================================================

fn match_query_parser<'src>() -> impl Parser<'src, &'src str, GraphIR, Extra<'src>> {
    let match_kw = kw("MATCH");
    let where_kw = kw("WHERE");
    let return_kw = kw("RETURN");

    match_kw
        .ignore_then(graph_pattern_parser())
        .then(where_clause_parser().or_not())
        .then(return_clause_parser().or_not())
        .map(|((pattern, where_clause), return_clause)| {
            build_match_ir(pattern, where_clause, return_clause)
        })
}

// ============================================================================
// Graph pattern: (node)-[edge]->(node) with quantifiers
// ============================================================================

fn graph_pattern_parser<'src>() -> impl Parser<'src, &'src str, Vec<PatternPart>, Extra<'src>> {
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

// ============================================================================
// Node pattern: (alias:Label {prop: value})
// ============================================================================

#[derive(Debug, Clone)]
struct NodePart {
    alias: Option<String>,
    label: Option<String>,
    properties: Vec<(String, CypherValue)>,
}

fn node_parser<'src>() -> impl Parser<'src, &'src str, NodePart, Extra<'src>> {
    let alias = ident().or_not();
    let label = just(':').ignore_then(ident()).or_not();
    let properties = properties_block().or_not().map(|o| o.unwrap_or_default());

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
        .labelled("node pattern like (a:Note {path: 'X'})")
}

// ============================================================================
// Edge pattern with quantifiers: -[:TYPE*1..3]->
// ============================================================================

#[derive(Debug, Clone)]
struct EdgePart {
    alias: Option<String>,
    edge_type: Option<String>,
    direction: EdgeDirection,
    quantifier: Option<Quantifier>,
}

fn edge_parser<'src>() -> impl Parser<'src, &'src str, EdgePart, Extra<'src>> {
    let edge_inner = edge_inner_parser();

    // Bidirectional: <-[...]->(highest priority)
    let bidirectional = just("<-")
        .ignore_then(edge_inner.clone())
        .then_ignore(just("->"))
        .map(|(alias, etype, quant)| EdgePart {
            alias,
            edge_type: etype,
            direction: EdgeDirection::Both,
            quantifier: quant,
        });

    // Incoming: <-[...]-
    let incoming = just("<-")
        .ignore_then(edge_inner.clone())
        .then_ignore(just('-'))
        .map(|(alias, etype, quant)| EdgePart {
            alias,
            edge_type: etype,
            direction: EdgeDirection::In,
            quantifier: quant,
        });

    // Outgoing: -[...]->
    let outgoing = just('-')
        .ignore_then(edge_inner.clone())
        .then_ignore(just("->"))
        .map(|(alias, etype, quant)| EdgePart {
            alias,
            edge_type: etype,
            direction: EdgeDirection::Out,
            quantifier: quant,
        });

    // Undirected: -[...]-
    let undirected = just('-')
        .ignore_then(edge_inner)
        .then_ignore(just('-'))
        .map(|(alias, etype, quant)| EdgePart {
            alias,
            edge_type: etype,
            direction: EdgeDirection::Undirected,
            quantifier: quant,
        });

    choice((bidirectional, incoming, outgoing, undirected)).padded()
}

/// Inner edge: [alias:TYPE*1..3]
fn edge_inner_parser<'src>(
) -> impl Parser<'src, &'src str, (Option<String>, Option<String>, Option<Quantifier>), Extra<'src>>
{
    let alias = ident().or_not();
    let edge_type = just(':').ignore_then(ident()).or_not();
    let quantifier = quantifier_parser().or_not();

    just('[')
        .padded()
        .ignore_then(alias)
        .then(edge_type)
        .then(quantifier)
        .then_ignore(just(']').padded())
        .map(|((alias, etype), quant)| (alias, etype, quant))
}

// ============================================================================
// Path quantifiers: *, *1..3, *..5, *2..
// ============================================================================

fn quantifier_parser<'src>() -> impl Parser<'src, &'src str, Quantifier, Extra<'src>> {
    let number = text::int(10).map(|s: &str| s.parse::<usize>().unwrap());

    // *1..3 (range with both bounds)
    let range_both = just('*')
        .ignore_then(number.clone())
        .then_ignore(just(".."))
        .then(number.clone())
        .map(|(min, max)| Quantifier::Range {
            min,
            max: Some(max),
        });

    // *..3 (range with max only, min=0)
    let range_max = just("*..")
        .ignore_then(number.clone())
        .map(|max| Quantifier::Range {
            min: 0,
            max: Some(max),
        });

    // *2.. (range with min only, no max)
    let range_min = just('*')
        .ignore_then(number.clone())
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
    // alias.property op value
    let field = ident()
        .then_ignore(just('.'))
        .then(ident())
        .map(|(alias, prop)| format!("{}.{}", alias, prop));

    // Comparison operators
    let op = choice((
        just("=").to(MatchOp::Eq),
        just("!=").or(just("<>")).to(MatchOp::Ne),
        just("STARTS WITH").to(MatchOp::StartsWith),
        just("ENDS WITH").to(MatchOp::EndsWith),
        just("CONTAINS").to(MatchOp::Contains),
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
    // alias.property AS name  OR  alias  OR  alias.property
    let field = ident()
        .then(just('.').ignore_then(ident()).or_not())
        .map(|(alias, prop)| match prop {
            Some(p) => format!("{}.{}", alias, p),
            None => alias,
        });

    let alias = kw("AS").ignore_then(ident()).or_not();

    field
        .padded()
        .then(alias)
        .map(|(field, alias)| Projection { field, alias })
}

// ============================================================================
// CREATE: CREATE (n:Label {props}) or CREATE (a)-[:TYPE]->(b)
// ============================================================================

fn create_query_parser<'src>() -> impl Parser<'src, &'src str, GraphIR, Extra<'src>> {
    kw("CREATE")
        .ignore_then(graph_pattern_parser())
        .map(|pattern| {
            let mut ir = build_match_ir(pattern, None, None);
            ir.mutation = Some(Mutation::Create);
            ir
        })
}

// ============================================================================
// DELETE: MATCH (n) WHERE ... DELETE n  or  DETACH DELETE n
// ============================================================================

fn delete_query_parser<'src>() -> impl Parser<'src, &'src str, GraphIR, Extra<'src>> {
    let detach = kw("DETACH").or_not().map(|o| o.is_some());

    kw("MATCH")
        .ignore_then(graph_pattern_parser())
        .then(where_clause_parser().or_not())
        .then_ignore(detach)
        .then_ignore(kw("DELETE"))
        .then(ident()) // What to delete
        .map(|((pattern, where_clause), target)| {
            let mut ir = build_match_ir(pattern, where_clause, None);
            ir.mutation = Some(Mutation::Delete { target });
            ir
        })
}

// ============================================================================
// Helpers
// ============================================================================

/// Case-insensitive keyword
fn kw<'src>(keyword: &'static str) -> impl Parser<'src, &'src str, (), Extra<'src>> {
    text::keyword::<&str, _, Extra<'src>>(keyword)
        .or(text::keyword(keyword.to_lowercase().leak()))
        .padded()
        .ignored()
}

/// Identifier: alphanumeric + underscore
fn ident<'src>() -> impl Parser<'src, &'src str, String, Extra<'src>> {
    any()
        .filter(|c: &char| c.is_alphanumeric() || *c == '_')
        .repeated()
        .at_least(1)
        .to_slice()
        .map(|s: &str| s.to_string())
        .padded()
}

/// Value literal (string, number, boolean, null, parameter)
fn value_parser<'src>() -> impl Parser<'src, &'src str, serde_json::Value, Extra<'src>> {
    let string = string_literal().map(serde_json::Value::String);

    let number = text::int(10)
        .then(just('.').then(text::digits(10)).or_not())
        .to_slice()
        .map(|s: &str| {
            if s.contains('.') {
                serde_json::Value::Number(
                    serde_json::Number::from_f64(s.parse().unwrap()).unwrap(),
                )
            } else {
                serde_json::Value::Number(s.parse::<i64>().unwrap().into())
            }
        });

    let boolean = choice((
        kw("true").to(serde_json::Value::Bool(true)),
        kw("false").to(serde_json::Value::Bool(false)),
    ));

    let null = kw("null").to(serde_json::Value::Null);

    // Parameter: $name (keep as string with $ prefix for substitution)
    let param = just('$')
        .ignore_then(ident())
        .map(|name| serde_json::Value::String(format!("${}", name)));

    choice((string, number, boolean, null, param)).padded()
}

fn string_literal<'src>() -> impl Parser<'src, &'src str, String, Extra<'src>> {
    let single = just('\'')
        .ignore_then(none_of("'").repeated().to_slice())
        .then_ignore(just('\''));

    let double = just('"')
        .ignore_then(none_of("\"").repeated().to_slice())
        .then_ignore(just('"'));

    single.or(double).map(|s: &str| s.to_string())
}

fn properties_block<'src>() -> impl Parser<'src, &'src str, Vec<(String, CypherValue)>, Extra<'src>>
{
    let property = ident()
        .then_ignore(just(':').padded())
        .then(value_parser());

    just('{')
        .padded()
        .ignore_then(
            property
                .separated_by(just(',').padded())
                .allow_trailing()
                .collect(),
        )
        .then_ignore(just('}').padded())
}

// ============================================================================
// IR construction
// ============================================================================

#[derive(Debug)]
enum PatternPart {
    Node(NodePart),
    Edge(EdgePart),
}

fn build_match_ir(
    parts: Vec<PatternPart>,
    where_clause: Option<Vec<Filter>>,
    return_clause: Option<Vec<Projection>>,
) -> GraphIR {
    let mut elements = Vec::new();
    let mut source = QuerySource::All;

    for part in parts {
        match part {
            PatternPart::Node(node) => {
                // Extract source from first node with path/title property
                for (key, value) in &node.properties {
                    if source == QuerySource::All {
                        if key == "path" {
                            if let serde_json::Value::String(s) = value {
                                source = QuerySource::ByPath(s.clone());
                            }
                        } else if key == "title" {
                            if let serde_json::Value::String(s) = value {
                                source = QuerySource::ByTitle(s.clone());
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
            PatternPart::Edge(edge) => {
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
        mutation: None, // Set by create/delete parsers
    }
}

fn format_errors(errs: &[Rich<'_, char>], input: &str) -> String {
    errs.iter()
        .map(|e| {
            let span = e.span();
            let start = span.start;
            let line = input[..start].lines().count().max(1);
            let col = start - input[..start].rfind('\n').map_or(0, |i| i + 1);
            format!("Line {}, col {}: {}", line, col + 1, e.reason())
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_match() {
        let syntax = CypherSyntax;
        let ir = syntax
            .parse("MATCH (n:Note {path: 'foo.md'}) RETURN n")
            .unwrap();

        assert_eq!(ir.source, QuerySource::ByPath("foo.md".to_string()));
        assert_eq!(ir.projections.len(), 1);
        assert_eq!(ir.projections[0].field, "n");
    }

    #[test]
    fn test_variable_length_path() {
        let syntax = CypherSyntax;
        let ir = syntax
            .parse("MATCH (a:Note {path: 'x'})-[:LINKS_TO*1..3]-(b) RETURN b.path")
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
    fn test_where_clause() {
        let syntax = CypherSyntax;
        let ir = syntax
            .parse("MATCH (n:Note) WHERE n.folder = 'Projects' RETURN n")
            .unwrap();

        assert_eq!(ir.filters.len(), 1);
        assert_eq!(ir.filters[0].field, "n.folder");
        assert_eq!(ir.filters[0].op, MatchOp::Eq);
    }

    #[test]
    fn test_parameter() {
        let syntax = CypherSyntax;
        let ir = syntax
            .parse("MATCH (n:Note {path: $path}) RETURN n")
            .unwrap();

        // Parameter stored as "$path" string for later substitution
        if let PatternElement::Node(node) = &ir.pattern.elements[0] {
            assert_eq!(
                node.properties[0].value,
                serde_json::Value::String("$path".to_string())
            );
        }
    }

    #[test]
    fn test_create_node() {
        let syntax = CypherSyntax;
        let ir = syntax
            .parse("CREATE (n:Note {path: 'new.md', title: 'New Note'})")
            .unwrap();

        assert!(matches!(ir.mutation, Some(Mutation::Create)));
    }

    #[test]
    fn test_delete_with_detach() {
        let syntax = CypherSyntax;
        let ir = syntax
            .parse("MATCH (n:Note {path: 'old.md'}) DETACH DELETE n")
            .unwrap();

        assert!(matches!(ir.mutation, Some(Mutation::Delete { .. })));
    }
}
