//! Query syntax parsers.
//!
//! This module defines the `QuerySyntax` trait and `QuerySyntaxRegistry`
//! for composable query parsing. Follows the SyntaxExtension pattern
//! from crucible-parser.

mod jaq;
mod pgq;
mod sql_sugar;

pub use jaq::JaqSyntax;
pub use pgq::PgqSyntax;
pub use sql_sugar::SqlSugarSyntax;

use crate::error::ParseError;
use crate::ir::GraphIR;
use std::sync::Arc;

/// Trait for query syntax parsers.
///
/// Follows the SyntaxExtension pattern from crucible-parser:
/// - `can_handle()` for fast detection
/// - `priority()` for ordering
/// - `parse()` for actual parsing
pub trait QuerySyntax: Send + Sync {
    /// Unique name for this syntax
    fn name(&self) -> &'static str;

    /// Fast check if this syntax might handle the input.
    ///
    /// Should be cheap (regex or prefix check). If true, `parse()` will be
    /// called. If false, the next syntax in priority order will be tried.
    fn can_handle(&self, input: &str) -> bool;

    /// Parse input into GraphIR.
    ///
    /// Called only if `can_handle()` returned true.
    fn parse(&self, input: &str) -> Result<GraphIR, ParseError>;

    /// Priority (higher = tried first). Default: 50
    fn priority(&self) -> u8 {
        50
    }
}

/// Registry of syntax parsers (sorted by priority descending).
///
/// The first syntax where `can_handle()` returns true will be used.
pub struct QuerySyntaxRegistry {
    syntaxes: Vec<Arc<dyn QuerySyntax>>,
}

impl Default for QuerySyntaxRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl QuerySyntaxRegistry {
    /// Create an empty registry
    pub fn new() -> Self {
        Self {
            syntaxes: Vec::new(),
        }
    }

    /// Register a syntax (re-sorts by priority)
    pub fn register(&mut self, syntax: Arc<dyn QuerySyntax>) {
        self.syntaxes.push(syntax);
        self.syntaxes
            .sort_by_key(|s| std::cmp::Reverse(s.priority()));
    }

    /// Parse using first matching syntax
    pub fn parse(&self, input: &str) -> Result<GraphIR, ParseError> {
        for syntax in &self.syntaxes {
            if syntax.can_handle(input) {
                return syntax.parse(input);
            }
        }
        Err(ParseError::NoMatchingSyntax {
            input: input.to_string(),
            tried: self.syntaxes.iter().map(|s| s.name()).collect(),
        })
    }

    /// Get list of registered syntax names
    pub fn syntax_names(&self) -> Vec<&'static str> {
        self.syntaxes.iter().map(|s| s.name()).collect()
    }
}

/// Builder for ergonomic registry construction
pub struct QuerySyntaxRegistryBuilder {
    syntaxes: Vec<Arc<dyn QuerySyntax>>,
}

impl Default for QuerySyntaxRegistryBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl QuerySyntaxRegistryBuilder {
    pub fn new() -> Self {
        Self {
            syntaxes: Vec::new(),
        }
    }

    /// Add a syntax to the registry
    pub fn with_syntax(mut self, syntax: impl QuerySyntax + 'static) -> Self {
        self.syntaxes.push(Arc::new(syntax));
        self
    }

    /// Build the registry
    pub fn build(self) -> QuerySyntaxRegistry {
        let mut registry = QuerySyntaxRegistry::new();
        for syntax in self.syntaxes {
            registry.register(syntax);
        }
        registry
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockSyntax {
        name: &'static str,
        priority: u8,
        prefix: &'static str,
    }

    impl QuerySyntax for MockSyntax {
        fn name(&self) -> &'static str {
            self.name
        }

        fn can_handle(&self, input: &str) -> bool {
            input.trim().to_lowercase().starts_with(self.prefix)
        }

        fn parse(&self, _input: &str) -> Result<GraphIR, ParseError> {
            Ok(GraphIR::default())
        }

        fn priority(&self) -> u8 {
            self.priority
        }
    }

    #[test]
    fn test_registry_priority_order() {
        let registry = QuerySyntaxRegistryBuilder::new()
            .with_syntax(MockSyntax {
                name: "low",
                priority: 10,
                prefix: "select",
            })
            .with_syntax(MockSyntax {
                name: "high",
                priority: 90,
                prefix: "select",
            })
            .with_syntax(MockSyntax {
                name: "medium",
                priority: 50,
                prefix: "select",
            })
            .build();

        // Should be sorted high -> medium -> low
        let names = registry.syntax_names();
        assert_eq!(names, vec!["high", "medium", "low"]);
    }

    #[test]
    fn test_registry_first_match_wins() {
        let registry = QuerySyntaxRegistryBuilder::new()
            .with_syntax(MockSyntax {
                name: "sql",
                priority: 50,
                prefix: "select",
            })
            .with_syntax(MockSyntax {
                name: "jaq",
                priority: 30,
                prefix: "outlinks",
            })
            .build();

        // Should use sql syntax (higher priority and matches)
        let result = registry.parse("SELECT outlinks FROM 'x'");
        assert!(result.is_ok());
    }

    #[test]
    fn test_registry_no_match() {
        let registry = QuerySyntaxRegistryBuilder::new()
            .with_syntax(MockSyntax {
                name: "sql",
                priority: 50,
                prefix: "select",
            })
            .build();

        let result = registry.parse("UNKNOWN query");
        assert!(matches!(result, Err(ParseError::NoMatchingSyntax { .. })));
    }
}
