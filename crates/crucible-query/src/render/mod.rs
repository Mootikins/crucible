//! Target renderers for GraphIR.
//!
//! Renderers convert the backend-agnostic GraphIR into target-specific
//! query strings (SurrealQL, DuckDB SQL, etc.).

mod surreal;

pub use surreal::SurrealRenderer;

use crate::error::RenderError;
use crate::ir::GraphIR;
use serde_json::Value;
use std::collections::HashMap;

/// Output from rendering
#[derive(Debug, Clone)]
pub struct RenderedQuery {
    /// The generated query string
    pub sql: String,
    /// Parameters to bind to the query
    pub params: HashMap<String, Value>,
}

/// Trait for rendering GraphIR to target query language.
pub trait QueryRenderer: Send + Sync {
    /// Unique name for this renderer
    fn name(&self) -> &str;

    /// Render the IR to a query string with parameters
    fn render(&self, ir: &GraphIR) -> Result<RenderedQuery, RenderError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockRenderer;

    impl QueryRenderer for MockRenderer {
        fn name(&self) -> &str {
            "mock"
        }

        fn render(&self, _ir: &GraphIR) -> Result<RenderedQuery, RenderError> {
            Ok(RenderedQuery {
                sql: "SELECT 1".to_string(),
                params: HashMap::new(),
            })
        }
    }

    #[test]
    fn test_mock_renderer() {
        let renderer = MockRenderer;
        let ir = GraphIR::default();
        let result = renderer.render(&ir).unwrap();

        assert_eq!(result.sql, "SELECT 1");
    }
}
