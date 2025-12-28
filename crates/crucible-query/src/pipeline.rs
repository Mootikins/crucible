//! Query pipeline orchestration.
//!
//! The QueryPipeline orchestrates the parse → transform → render phases.
//! Follows the NotePipeline pattern from crucible-pipeline.

use crate::error::PipelineError;
use crate::render::{QueryRenderer, RenderedQuery};
use crate::syntax::QuerySyntaxRegistry;
use crate::transform::QueryTransform;
use std::sync::Arc;

/// Orchestrates parsing → transform → rendering.
///
/// Follows the NotePipeline pattern from crucible-pipeline.
pub struct QueryPipeline {
    syntax_registry: QuerySyntaxRegistry,
    transforms: Vec<Arc<dyn QueryTransform>>,
    renderer: Arc<dyn QueryRenderer>,
}

impl QueryPipeline {
    /// Create a new pipeline with the given components
    pub fn new(
        syntax_registry: QuerySyntaxRegistry,
        transforms: Vec<Arc<dyn QueryTransform>>,
        renderer: Arc<dyn QueryRenderer>,
    ) -> Self {
        Self {
            syntax_registry,
            transforms,
            renderer,
        }
    }

    /// Execute query through the pipeline
    pub fn execute(&self, query: &str) -> Result<RenderedQuery, PipelineError> {
        // Phase 1: Parse (first matching syntax wins)
        let mut ir = self.syntax_registry.parse(query)?;

        // Phase 2: Transform (apply in sequence)
        for transform in &self.transforms {
            ir = transform.transform(ir)?;
        }

        // Phase 3: Render to target
        let rendered = self.renderer.render(&ir)?;

        Ok(rendered)
    }

    /// Get registered syntax names
    pub fn syntax_names(&self) -> Vec<&'static str> {
        self.syntax_registry.syntax_names()
    }
}

/// Builder for pipeline construction
pub struct QueryPipelineBuilder {
    syntax_registry: Option<QuerySyntaxRegistry>,
    transforms: Vec<Arc<dyn QueryTransform>>,
    renderer: Option<Arc<dyn QueryRenderer>>,
}

impl Default for QueryPipelineBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl QueryPipelineBuilder {
    pub fn new() -> Self {
        Self {
            syntax_registry: None,
            transforms: Vec::new(),
            renderer: None,
        }
    }

    /// Set the syntax registry
    pub fn syntax_registry(mut self, registry: QuerySyntaxRegistry) -> Self {
        self.syntax_registry = Some(registry);
        self
    }

    /// Add a transform to the pipeline
    pub fn transform(mut self, t: impl QueryTransform + 'static) -> Self {
        self.transforms.push(Arc::new(t));
        self
    }

    /// Set the renderer
    pub fn renderer(mut self, r: impl QueryRenderer + 'static) -> Self {
        self.renderer = Some(Arc::new(r));
        self
    }

    /// Build the pipeline
    ///
    /// # Panics
    ///
    /// Panics if syntax_registry or renderer was not set.
    pub fn build(self) -> QueryPipeline {
        QueryPipeline::new(
            self.syntax_registry.expect("syntax_registry required"),
            self.transforms,
            self.renderer.expect("renderer required"),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::SurrealRenderer;
    use crate::syntax::{QuerySyntaxRegistryBuilder, SqlSugarSyntax};
    use crate::transform::ValidateTransform;

    #[test]
    fn test_pipeline_execute() {
        let syntax_registry = QuerySyntaxRegistryBuilder::new()
            .with_syntax(SqlSugarSyntax)
            .build();

        let pipeline = QueryPipelineBuilder::new()
            .syntax_registry(syntax_registry)
            .transform(ValidateTransform)
            .renderer(SurrealRenderer::default())
            .build();

        let result = pipeline.execute("SELECT outlinks FROM 'Index'").unwrap();

        assert!(result.sql.contains("SELECT"));
        assert!(result.sql.contains("FETCH out"));
    }

    #[test]
    fn test_pipeline_no_matching_syntax() {
        let syntax_registry = QuerySyntaxRegistryBuilder::new()
            .with_syntax(SqlSugarSyntax)
            .build();

        let pipeline = QueryPipelineBuilder::new()
            .syntax_registry(syntax_registry)
            .renderer(SurrealRenderer::default())
            .build();

        let result = pipeline.execute("UNKNOWN query");

        assert!(matches!(result, Err(PipelineError::Parse(_))));
    }
}
