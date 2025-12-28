//! jaq-style query syntax parser.
//!
//! Parses queries like:
//! - `outlinks("Index")`
//! - `inlinks("Project")`
//! - `find("Note")`
//! - `neighbors("Hub")`
//! - `find("Note") | ->wikilink[]`
//! - `find("Note") | ->wikilink[] | select(.tags)`
//!
//! Priority: 30 (lowest, fallback)

use crate::error::ParseError;
use crate::ir::{EdgeDirection, EdgePattern, GraphIR, GraphPattern, PatternElement, QuerySource};
use crate::syntax::QuerySyntax;
use once_cell::sync::Lazy;
use regex::Regex;

/// Pattern for jaq-style function calls at start
static JAQ_FUNCTION_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"^\s*(outlinks|inlinks|find|neighbors)\s*\("#).unwrap());

/// Pattern for arrow traversals: ->edge[], <-edge[], <->edge[]
static ARROW_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^(<->|->|<-)(\w+)\[\]$").unwrap());

/// jaq-style syntax parser
pub struct JaqSyntax;

impl QuerySyntax for JaqSyntax {
    fn name(&self) -> &'static str {
        "jaq"
    }

    fn can_handle(&self, input: &str) -> bool {
        JAQ_FUNCTION_RE.is_match(input)
    }

    fn parse(&self, input: &str) -> Result<GraphIR, ParseError> {
        let input = input.trim();

        // Split by pipes to handle hybrid queries
        let segments: Vec<&str> = input.split('|').map(|s| s.trim()).collect();

        let mut traversals = Vec::new();
        let mut source = None;
        let mut post_filter_parts = Vec::new();

        for segment in segments {
            // Check for graph function at start
            if source.is_none() {
                if let Some((func, title)) = self.parse_graph_function(segment)? {
                    source = Some((func, title));
                    continue;
                }
            }

            // Check for arrow traversal
            if let Some(edge) = self.parse_arrow(segment)? {
                traversals.push(edge);
                continue;
            }

            // Everything else is post-filter (jaq expression)
            post_filter_parts.push(segment);
        }

        let (func, title) = source.ok_or_else(|| ParseError::Jaq {
            message: "Query must start with a graph function (outlinks, inlinks, find, neighbors)"
                .to_string(),
        })?;

        // Build IR based on function type
        let (query_source, pattern) = match func.as_str() {
            "find" => (QuerySource::ByTitle(title), GraphPattern::default()),
            "outlinks" => (
                QuerySource::ByTitle(title),
                GraphPattern {
                    elements: vec![PatternElement::Edge(EdgePattern {
                        direction: EdgeDirection::Out,
                        edge_type: Some("wikilink".to_string()),
                        ..Default::default()
                    })],
                },
            ),
            "inlinks" => (
                QuerySource::ByTitle(title),
                GraphPattern {
                    elements: vec![PatternElement::Edge(EdgePattern {
                        direction: EdgeDirection::In,
                        edge_type: Some("wikilink".to_string()),
                        ..Default::default()
                    })],
                },
            ),
            "neighbors" => (
                QuerySource::ByTitle(title),
                GraphPattern {
                    elements: vec![PatternElement::Edge(EdgePattern {
                        direction: EdgeDirection::Both,
                        edge_type: Some("wikilink".to_string()),
                        ..Default::default()
                    })],
                },
            ),
            _ => {
                return Err(ParseError::Jaq {
                    message: format!("Unknown function: {}", func),
                })
            }
        };

        // Add additional traversals
        let mut pattern = pattern;
        for edge in traversals {
            pattern.elements.push(PatternElement::Edge(edge));
        }

        let post_filter = if post_filter_parts.is_empty() {
            None
        } else {
            Some(post_filter_parts.join(" | "))
        };

        Ok(GraphIR {
            source: query_source,
            pattern,
            projections: Vec::new(),
            filters: Vec::new(),
            post_filter,
        })
    }

    fn priority(&self) -> u8 {
        30 // Lowest - fallback
    }
}

impl JaqSyntax {
    /// Parse a graph function call: func("arg")
    ///
    /// Returns (function_name, argument) if successful
    fn parse_graph_function(&self, segment: &str) -> Result<Option<(String, String)>, ParseError> {
        let segment = segment.trim();

        let patterns = ["outlinks", "inlinks", "find", "neighbors"];

        for name in patterns {
            if let Some(rest) = segment.strip_prefix(name) {
                let rest = rest.trim();
                if let Some(arg) = self.extract_string_arg(rest)? {
                    return Ok(Some((name.to_string(), arg)));
                }
            }
        }

        Ok(None)
    }

    /// Extract a string argument from parentheses: ("value") -> value
    ///
    /// Only matches simple function calls, not function calls followed by more expressions.
    fn extract_string_arg(&self, s: &str) -> Result<Option<String>, ParseError> {
        let s = s.trim();

        if !s.starts_with('(') {
            return Ok(None);
        }

        let s = s.strip_prefix('(').unwrap().trim();

        // Look for quoted string
        let (quote_char, rest) = if s.starts_with('"') {
            ('"', s.strip_prefix('"').unwrap())
        } else if s.starts_with('\'') {
            ('\'', s.strip_prefix('\'').unwrap())
        } else {
            return Err(ParseError::Jaq {
                message: "Expected quoted string argument".to_string(),
            });
        };

        // Find closing quote
        if let Some(end) = rest.find(quote_char) {
            let arg = rest[..end].to_string();
            let remaining = rest[end + 1..].trim();

            // Should end with )
            if !remaining.starts_with(')') {
                return Err(ParseError::Jaq {
                    message: "Expected closing parenthesis".to_string(),
                });
            }

            // Check there's nothing after the closing paren
            let after_paren = remaining[1..].trim();
            if !after_paren.is_empty() {
                // There's more after the function call - this is a compound expression
                return Ok(None);
            }

            Ok(Some(arg))
        } else {
            Err(ParseError::Jaq {
                message: "Unclosed string argument".to_string(),
            })
        }
    }

    /// Parse arrow syntax: ->edge[], <-edge[], <->edge[]
    fn parse_arrow(&self, segment: &str) -> Result<Option<EdgePattern>, ParseError> {
        let segment = segment.trim();

        if let Some(caps) = ARROW_RE.captures(segment) {
            let direction = match &caps[1] {
                "->" => EdgeDirection::Out,
                "<-" => EdgeDirection::In,
                "<->" => EdgeDirection::Both,
                _ => {
                    return Err(ParseError::Jaq {
                        message: "Invalid arrow direction".to_string(),
                    })
                }
            };
            let edge_type = caps[2].to_string();

            return Ok(Some(EdgePattern {
                direction,
                edge_type: Some(edge_type),
                ..Default::default()
            }));
        }

        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // can_handle tests
    // =========================================================================

    #[test]
    fn test_can_handle_outlinks() {
        let syntax = JaqSyntax;
        assert!(syntax.can_handle(r#"outlinks("Index")"#));
    }

    #[test]
    fn test_can_handle_inlinks() {
        let syntax = JaqSyntax;
        assert!(syntax.can_handle(r#"inlinks("Project")"#));
    }

    #[test]
    fn test_can_handle_find() {
        let syntax = JaqSyntax;
        assert!(syntax.can_handle(r#"find("Note")"#));
    }

    #[test]
    fn test_can_handle_neighbors() {
        let syntax = JaqSyntax;
        assert!(syntax.can_handle(r#"neighbors("Hub")"#));
    }

    #[test]
    fn test_cannot_handle_sql() {
        let syntax = JaqSyntax;
        assert!(!syntax.can_handle("SELECT outlinks FROM 'Index'"));
    }

    #[test]
    fn test_cannot_handle_match() {
        let syntax = JaqSyntax;
        assert!(!syntax.can_handle("MATCH (a)-[:wikilink]->(b)"));
    }

    #[test]
    fn test_priority_is_low() {
        let syntax = JaqSyntax;
        assert_eq!(syntax.priority(), 30);
    }

    // =========================================================================
    // parse tests - simple function calls
    // =========================================================================

    #[test]
    fn test_parse_outlinks() {
        let syntax = JaqSyntax;
        let ir = syntax.parse(r#"outlinks("Index")"#).unwrap();

        assert_eq!(ir.source, QuerySource::ByTitle("Index".to_string()));
        assert_eq!(ir.pattern.elements.len(), 1);

        if let PatternElement::Edge(edge) = &ir.pattern.elements[0] {
            assert_eq!(edge.direction, EdgeDirection::Out);
            assert_eq!(edge.edge_type, Some("wikilink".to_string()));
        } else {
            panic!("Expected edge pattern");
        }
    }

    #[test]
    fn test_parse_inlinks() {
        let syntax = JaqSyntax;
        let ir = syntax.parse(r#"inlinks("Project")"#).unwrap();

        assert_eq!(ir.source, QuerySource::ByTitle("Project".to_string()));

        if let PatternElement::Edge(edge) = &ir.pattern.elements[0] {
            assert_eq!(edge.direction, EdgeDirection::In);
        } else {
            panic!("Expected edge pattern");
        }
    }

    #[test]
    fn test_parse_find() {
        let syntax = JaqSyntax;
        let ir = syntax.parse(r#"find("MyNote")"#).unwrap();

        assert_eq!(ir.source, QuerySource::ByTitle("MyNote".to_string()));
        assert!(ir.pattern.elements.is_empty());
    }

    #[test]
    fn test_parse_neighbors() {
        let syntax = JaqSyntax;
        let ir = syntax.parse(r#"neighbors("Hub")"#).unwrap();

        if let PatternElement::Edge(edge) = &ir.pattern.elements[0] {
            assert_eq!(edge.direction, EdgeDirection::Both);
        } else {
            panic!("Expected edge pattern");
        }
    }

    #[test]
    fn test_parse_with_single_quotes() {
        let syntax = JaqSyntax;
        let ir = syntax.parse(r#"outlinks('Index')"#).unwrap();

        assert_eq!(ir.source, QuerySource::ByTitle("Index".to_string()));
    }

    // =========================================================================
    // parse tests - hybrid queries with arrows
    // =========================================================================

    #[test]
    fn test_parse_find_with_arrow() {
        let syntax = JaqSyntax;
        let ir = syntax.parse(r#"find("Index") | ->wikilink[]"#).unwrap();

        assert_eq!(ir.source, QuerySource::ByTitle("Index".to_string()));
        // find() has no implicit traversal, but we added one
        assert_eq!(ir.pattern.elements.len(), 1);

        if let PatternElement::Edge(edge) = &ir.pattern.elements[0] {
            assert_eq!(edge.direction, EdgeDirection::Out);
            assert_eq!(edge.edge_type, Some("wikilink".to_string()));
        } else {
            panic!("Expected edge pattern");
        }
    }

    #[test]
    fn test_parse_with_multiple_arrows() {
        let syntax = JaqSyntax;
        let ir = syntax.parse(r#"find("Index") | ->wikilink[] | ->embed[]"#).unwrap();

        assert_eq!(ir.pattern.elements.len(), 2);

        if let PatternElement::Edge(edge1) = &ir.pattern.elements[0] {
            assert_eq!(edge1.edge_type, Some("wikilink".to_string()));
        }
        if let PatternElement::Edge(edge2) = &ir.pattern.elements[1] {
            assert_eq!(edge2.edge_type, Some("embed".to_string()));
        }
    }

    #[test]
    fn test_parse_with_incoming_arrow() {
        let syntax = JaqSyntax;
        let ir = syntax.parse(r#"find("Index") | <-wikilink[]"#).unwrap();

        if let PatternElement::Edge(edge) = &ir.pattern.elements[0] {
            assert_eq!(edge.direction, EdgeDirection::In);
        } else {
            panic!("Expected edge pattern");
        }
    }

    #[test]
    fn test_parse_with_bidirectional_arrow() {
        let syntax = JaqSyntax;
        let ir = syntax.parse(r#"find("Index") | <->wikilink[]"#).unwrap();

        if let PatternElement::Edge(edge) = &ir.pattern.elements[0] {
            assert_eq!(edge.direction, EdgeDirection::Both);
        } else {
            panic!("Expected edge pattern");
        }
    }

    // =========================================================================
    // parse tests - post-filter preservation
    // =========================================================================

    #[test]
    fn test_parse_with_jaq_filter() {
        let syntax = JaqSyntax;
        let ir = syntax.parse(r#"outlinks("Index") | select(.tags)"#).unwrap();

        assert_eq!(ir.post_filter, Some("select(.tags)".to_string()));
    }

    #[test]
    fn test_parse_with_complex_filter() {
        let syntax = JaqSyntax;
        let ir = syntax.parse(r#"find("Index") | ->wikilink[] | select(.tags | contains("project"))"#).unwrap();

        assert_eq!(ir.pattern.elements.len(), 1);
        assert_eq!(
            ir.post_filter,
            Some(r#"select(.tags | contains("project"))"#.to_string())
        );
    }

    // =========================================================================
    // parse tests - edge cases
    // =========================================================================

    #[test]
    fn test_parse_empty_title() {
        let syntax = JaqSyntax;
        let ir = syntax.parse(r#"outlinks("")"#).unwrap();

        assert_eq!(ir.source, QuerySource::ByTitle("".to_string()));
    }

    #[test]
    fn test_parse_unicode_title() {
        let syntax = JaqSyntax;
        let ir = syntax.parse(r#"outlinks("日本語ノート")"#).unwrap();

        assert_eq!(ir.source, QuerySource::ByTitle("日本語ノート".to_string()));
    }

    #[test]
    fn test_parse_error_no_function() {
        let syntax = JaqSyntax;
        let result = syntax.parse("->wikilink[]");

        assert!(matches!(result, Err(ParseError::Jaq { .. })));
    }

    #[test]
    fn test_parse_error_unknown_function() {
        let syntax = JaqSyntax;
        let result = syntax.parse(r#"unknown("Index")"#);

        assert!(matches!(result, Err(ParseError::Jaq { .. })));
    }
}
