//! Layered Prompt Builder
//!
//! Assembles system prompts from multiple sources with clear separation.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A layer in the prompt hierarchy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptLayer {
    /// Name of this layer (e.g., "system", "personality", "task")
    pub name: String,

    /// The prompt content
    pub content: String,

    /// Priority for ordering (higher = later in final prompt)
    pub priority: u32,

    /// Whether this layer can be omitted if space is tight
    pub optional: bool,
}

impl PromptLayer {
    /// Create a new prompt layer
    pub fn new(name: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            content: content.into(),
            priority: 100,
            optional: false,
        }
    }

    /// Set the priority for this layer
    pub fn with_priority(mut self, priority: u32) -> Self {
        self.priority = priority;
        self
    }

    /// Mark this layer as optional
    pub fn optional(mut self) -> Self {
        self.optional = true;
        self
    }

    /// Estimate token count for this layer
    pub fn estimated_tokens(&self) -> usize {
        // Simple heuristic: 4 chars per token
        self.content.len() / 4
    }
}

/// Builder for assembling layered prompts
///
/// Combines multiple prompt sources (global config, agent card, runtime context)
/// into a single coherent system prompt with clear layer separation.
#[derive(Debug, Default)]
pub struct LayeredPromptBuilder {
    layers: HashMap<String, PromptLayer>,
    separator: String,
}

impl LayeredPromptBuilder {
    /// Create a new prompt builder
    pub fn new() -> Self {
        Self {
            layers: HashMap::new(),
            separator: "\n\n---\n\n".to_string(),
        }
    }

    /// Set a custom separator between layers
    pub fn with_separator(mut self, separator: impl Into<String>) -> Self {
        self.separator = separator.into();
        self
    }

    /// Add a layer to the prompt
    pub fn add_layer(&mut self, layer: PromptLayer) -> &mut Self {
        self.layers.insert(layer.name.clone(), layer);
        self
    }

    /// Add a simple text layer
    pub fn add_text(&mut self, name: impl Into<String>, content: impl Into<String>) -> &mut Self {
        let layer = PromptLayer::new(name, content);
        self.layers.insert(layer.name.clone(), layer);
        self
    }

    /// Remove a layer by name
    pub fn remove_layer(&mut self, name: &str) -> Option<PromptLayer> {
        self.layers.remove(name)
    }

    /// Check if a layer exists
    pub fn has_layer(&self, name: &str) -> bool {
        self.layers.contains_key(name)
    }

    /// Get a layer by name
    pub fn get_layer(&self, name: &str) -> Option<&PromptLayer> {
        self.layers.get(name)
    }

    /// Get a mutable reference to a layer
    pub fn get_layer_mut(&mut self, name: &str) -> Option<&mut PromptLayer> {
        self.layers.get_mut(name)
    }

    /// Build the final prompt from all layers
    ///
    /// Layers are ordered by priority (lowest to highest).
    /// If `max_tokens` is provided, optional layers may be omitted to fit.
    pub fn build(&self, _max_tokens: Option<usize>) -> String {
        todo!("Implement LayeredPromptBuilder::build")
    }

    /// Estimate total token count for all layers
    pub fn estimated_tokens(&self) -> usize {
        self.layers.values().map(|l| l.estimated_tokens()).sum()
    }

    /// Get all layer names
    pub fn layer_names(&self) -> Vec<&str> {
        self.layers.keys().map(|s| s.as_str()).collect()
    }

    /// Clear all layers
    pub fn clear(&mut self) {
        self.layers.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_layer() {
        let layer = PromptLayer::new("system", "You are a helpful assistant");
        assert_eq!(layer.name, "system");
        assert_eq!(layer.content, "You are a helpful assistant");
        assert_eq!(layer.priority, 100);
        assert!(!layer.optional);
    }

    #[test]
    fn test_layer_priority() {
        let layer = PromptLayer::new("task", "Do the thing").with_priority(200);
        assert_eq!(layer.priority, 200);
    }

    #[test]
    fn test_layer_optional() {
        let layer = PromptLayer::new("context", "Extra info").optional();
        assert!(layer.optional);
    }

    #[test]
    fn test_layer_token_estimate() {
        let layer = PromptLayer::new("test", "a".repeat(400));
        assert_eq!(layer.estimated_tokens(), 100);
    }

    #[test]
    fn test_builder_add_layer() {
        let mut builder = LayeredPromptBuilder::new();
        builder.add_layer(PromptLayer::new("system", "System prompt"));

        assert!(builder.has_layer("system"));
        assert_eq!(builder.layer_names().len(), 1);
    }

    #[test]
    fn test_builder_add_text() {
        let mut builder = LayeredPromptBuilder::new();
        builder.add_text("task", "Do something");

        assert!(builder.has_layer("task"));
    }

    #[test]
    fn test_builder_remove_layer() {
        let mut builder = LayeredPromptBuilder::new();
        builder.add_text("temp", "Temporary");

        let removed = builder.remove_layer("temp");
        assert!(removed.is_some());
        assert!(!builder.has_layer("temp"));
    }

    #[test]
    fn test_builder_get_layer() {
        let mut builder = LayeredPromptBuilder::new();
        builder.add_text("test", "Test content");

        let layer = builder.get_layer("test");
        assert!(layer.is_some());
        assert_eq!(layer.unwrap().content, "Test content");
    }

    #[test]
    fn test_builder_clear() {
        let mut builder = LayeredPromptBuilder::new();
        builder.add_text("a", "A");
        builder.add_text("b", "B");

        builder.clear();
        assert_eq!(builder.layer_names().len(), 0);
    }

    #[test]
    fn test_estimated_tokens() {
        let mut builder = LayeredPromptBuilder::new();
        builder.add_text("a", "a".repeat(400));
        builder.add_text("b", "b".repeat(400));

        assert_eq!(builder.estimated_tokens(), 200);
    }

    #[test]
    fn test_build() {
        // TODO: Implement when build() is ready
    }
}
