//! SQL sugar syntax parser.
//!
//! Parses simplified SQL-like syntax:
//! - `SELECT outlinks FROM 'Title'`
//! - `SELECT inlinks FROM 'Title'`
//! - `SELECT neighbors FROM 'Title'`
//! - `SELECT * FROM notes WHERE title = 'Title'`
//!
//! Priority: 40 (between PGQ and jaq)

use crate::error::ParseError;
use crate::ir::{EdgeDirection, EdgePattern, GraphIR, PatternElement, QuerySource};
use crate::syntax::QuerySyntax;
use once_cell::sync::Lazy;
use regex::Regex;

/// Pattern: SELECT outlinks/inlinks/neighbors FROM 'title'
static SQL_GRAPH_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?i)^\s*SELECT\s+(outlinks|inlinks|neighbors)\s+FROM\s+['"]([^'"]+)['"]\s*$"#)
        .unwrap()
});

/// Pattern: SELECT * FROM table WHERE title = 'title'
static SQL_FIND_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?i)^\s*SELECT\s+\*\s+FROM\s+\w+\s+WHERE\s+title\s*=\s*['"]([^'"]+)['"]\s*$"#)
        .unwrap()
});

/// SQL sugar syntax parser
pub struct SqlSugarSyntax;

impl QuerySyntax for SqlSugarSyntax {
    fn name(&self) -> &'static str {
        "sql-sugar"
    }

    fn can_handle(&self, input: &str) -> bool {
        SQL_GRAPH_RE.is_match(input) || SQL_FIND_RE.is_match(input)
    }

    fn parse(&self, input: &str) -> Result<GraphIR, ParseError> {
        // Try SELECT outlinks/inlinks/neighbors FROM 'title'
        if let Some(caps) = SQL_GRAPH_RE.captures(input) {
            let func = caps[1].to_lowercase();
            let title = caps[2].to_string();

            let direction = match func.as_str() {
                "outlinks" => EdgeDirection::Out,
                "inlinks" => EdgeDirection::In,
                "neighbors" => EdgeDirection::Both,
                _ => {
                    return Err(ParseError::SqlAlias {
                        message: format!("Unknown function: {}", func),
                    })
                }
            };

            return Ok(GraphIR {
                source: QuerySource::ByTitle(title),
                pattern: crate::ir::GraphPattern {
                    elements: vec![PatternElement::Edge(EdgePattern {
                        direction,
                        edge_type: Some("wikilink".to_string()),
                        ..Default::default()
                    })],
                },
                ..Default::default()
            });
        }

        // Try SELECT * FROM table WHERE title = 'title'
        if let Some(caps) = SQL_FIND_RE.captures(input) {
            let title = caps[1].to_string();

            return Ok(GraphIR {
                source: QuerySource::ByTitle(title),
                ..Default::default()
            });
        }

        Err(ParseError::SqlAlias {
            message: "Input does not match SQL sugar patterns".to_string(),
        })
    }

    fn priority(&self) -> u8 {
        40 // Between PGQ (50) and jaq (30)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_can_handle_select_outlinks() {
        let syntax = SqlSugarSyntax;
        assert!(syntax.can_handle("SELECT outlinks FROM 'Index'"));
    }

    #[test]
    fn test_can_handle_select_inlinks() {
        let syntax = SqlSugarSyntax;
        assert!(syntax.can_handle("SELECT inlinks FROM 'Project'"));
    }

    #[test]
    fn test_can_handle_select_neighbors() {
        let syntax = SqlSugarSyntax;
        assert!(syntax.can_handle("SELECT neighbors FROM 'Hub'"));
    }

    #[test]
    fn test_can_handle_select_star_where() {
        let syntax = SqlSugarSyntax;
        assert!(syntax.can_handle("SELECT * FROM notes WHERE title = 'MyNote'"));
    }

    #[test]
    fn test_can_handle_case_insensitive() {
        let syntax = SqlSugarSyntax;
        assert!(syntax.can_handle("select OUTLINKS from 'Index'"));
    }

    #[test]
    fn test_cannot_handle_match() {
        let syntax = SqlSugarSyntax;
        assert!(!syntax.can_handle("MATCH (a)-[:wikilink]->(b)"));
    }

    #[test]
    fn test_cannot_handle_jaq() {
        let syntax = SqlSugarSyntax;
        assert!(!syntax.can_handle(r#"outlinks("Index")"#));
    }

    #[test]
    fn test_parse_outlinks() {
        let syntax = SqlSugarSyntax;
        let ir = syntax.parse("SELECT outlinks FROM 'Index'").unwrap();

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
        let syntax = SqlSugarSyntax;
        let ir = syntax.parse("SELECT inlinks FROM 'Project'").unwrap();

        assert_eq!(ir.source, QuerySource::ByTitle("Project".to_string()));

        if let PatternElement::Edge(edge) = &ir.pattern.elements[0] {
            assert_eq!(edge.direction, EdgeDirection::In);
        } else {
            panic!("Expected edge pattern");
        }
    }

    #[test]
    fn test_parse_neighbors() {
        let syntax = SqlSugarSyntax;
        let ir = syntax.parse("SELECT neighbors FROM 'Hub'").unwrap();

        if let PatternElement::Edge(edge) = &ir.pattern.elements[0] {
            assert_eq!(edge.direction, EdgeDirection::Both);
        } else {
            panic!("Expected edge pattern");
        }
    }

    #[test]
    fn test_parse_find() {
        let syntax = SqlSugarSyntax;
        let ir = syntax
            .parse("SELECT * FROM notes WHERE title = 'MyNote'")
            .unwrap();

        assert_eq!(ir.source, QuerySource::ByTitle("MyNote".to_string()));
        assert!(ir.pattern.elements.is_empty());
    }

    #[test]
    fn test_priority_is_medium() {
        let syntax = SqlSugarSyntax;
        assert_eq!(syntax.priority(), 40);
    }
}
