//! Layered Prompt Builder
//!
//! Assembles system prompts from multiple sources with clear separation.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// A layer in the prompt hierarchy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptLayer {
    /// The prompt content
    pub content: String,

    /// Priority for ordering (higher = later in final prompt)
    pub priority: u32,

    /// Whether this layer can be omitted if space is tight
    pub optional: bool,
}

impl PromptLayer {
    /// Create a new prompt layer with content
    pub fn new(content: impl Into<String>) -> Self {
        Self {
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
///
/// # Layer Priority Order
///
/// The prompt layering stacks from bottom to top:
/// 1. Base prompt (minimal default) - priority 100
/// 2. AGENTS.md / CLAUDE.md (if present in cwd) - priority 200
/// 3. Agent card system prompt (if specified) - priority 300
/// 4. User customization (future) - priority 400
#[derive(Debug)]
pub struct LayeredPromptBuilder {
    layers: HashMap<String, PromptLayer>,
    separator: String,
}

impl Default for LayeredPromptBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl LayeredPromptBuilder {
    /// Create a new prompt builder with default base prompt
    pub fn new() -> Self {
        let mut builder = Self {
            layers: HashMap::new(),
            separator: "\n\n---\n\n".to_string(),
        };
        // Add base prompt by default
        builder.add_layer_internal(
            "base",
            PromptLayer::new("You are a helpful assistant.").with_priority(100),
        );
        builder
    }

    /// Set a custom separator between layers
    pub fn with_separator(mut self, separator: impl Into<String>) -> Self {
        self.separator = separator.into();
        self
    }

    /// Load AGENTS.md or CLAUDE.md from directory
    ///
    /// Tries AGENTS.md first, falls back to CLAUDE.md if not found.
    pub fn with_agents_md(mut self, dir: &Path) -> Self {
        let agents_path = dir.join("AGENTS.md");
        let claude_path = dir.join("CLAUDE.md");

        if let Ok(content) = fs::read_to_string(&agents_path) {
            self.add_layer_internal("agents_md", PromptLayer::new(content).with_priority(200));
        } else if let Ok(content) = fs::read_to_string(&claude_path) {
            self.add_layer_internal("agents_md", PromptLayer::new(content).with_priority(200));
        }
        self
    }

    /// Add agent card system prompt
    pub fn with_agent_card(mut self, system_prompt: impl Into<String>) -> Self {
        self.add_layer_internal(
            "agent_card",
            PromptLayer::new(system_prompt).with_priority(300),
        );
        self
    }

    /// Add user customization prompt
    pub fn with_user_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.add_layer_internal("user", PromptLayer::new(prompt).with_priority(400));
        self
    }

    /// Internal method to add a layer with a name
    fn add_layer_internal(&mut self, name: impl Into<String>, layer: PromptLayer) {
        self.layers.insert(name.into(), layer);
    }

    /// Add a layer to the prompt (deprecated - use specific methods)
    #[deprecated(note = "Use specific methods like with_agent_card() instead")]
    pub fn add_layer(&mut self, name: impl Into<String>, layer: PromptLayer) -> &mut Self {
        self.layers.insert(name.into(), layer);
        self
    }

    /// Add a simple text layer (deprecated - use specific methods)
    #[deprecated(note = "Use specific methods like with_agent_card() instead")]
    pub fn add_text(&mut self, name: impl Into<String>, content: impl Into<String>) -> &mut Self {
        let layer = PromptLayer::new(content);
        self.layers.insert(name.into(), layer);
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
    /// Empty layers and optional layers with empty content are filtered out.
    pub fn build(&self) -> String {
        let mut layers: Vec<_> = self.layers.values().collect();
        layers.sort_by_key(|l| l.priority);

        layers
            .iter()
            .filter(|l| !l.content.is_empty())
            .filter(|l| !l.optional || !l.content.trim().is_empty())
            .map(|l| l.content.as_str())
            .collect::<Vec<_>>()
            .join(&self.separator)
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
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_new_layer() {
        let layer = PromptLayer::new("You are a helpful assistant");
        assert_eq!(layer.content, "You are a helpful assistant");
        assert_eq!(layer.priority, 100);
        assert!(!layer.optional);
    }

    #[test]
    fn test_layer_priority() {
        let layer = PromptLayer::new("Do the thing").with_priority(200);
        assert_eq!(layer.priority, 200);
    }

    #[test]
    fn test_layer_optional() {
        let layer = PromptLayer::new("Extra info").optional();
        assert!(layer.optional);
    }

    #[test]
    fn test_layer_token_estimate() {
        let layer = PromptLayer::new("a".repeat(400));
        assert_eq!(layer.estimated_tokens(), 100);
    }

    #[test]
    fn test_default_has_base_prompt() {
        let builder = LayeredPromptBuilder::new();
        assert!(builder.has_layer("base"));
        let base = builder.get_layer("base").unwrap();
        assert_eq!(base.priority, 100);
        assert_eq!(base.content, "You are a helpful assistant.");
    }

    #[test]
    fn test_layers_sorted_by_priority() {
        let builder = LayeredPromptBuilder::new()
            .with_agent_card("Agent card prompt")
            .with_user_prompt("User customization");

        let result = builder.build();

        // Should be: base (100) -> agent_card (300) -> user (400)
        let base_pos = result.find("You are a helpful assistant").unwrap();
        let agent_pos = result.find("Agent card prompt").unwrap();
        let user_pos = result.find("User customization").unwrap();

        assert!(base_pos < agent_pos);
        assert!(agent_pos < user_pos);
    }

    #[test]
    fn test_agents_md_loading() {
        let temp_dir = TempDir::new().unwrap();
        let agents_md = temp_dir.path().join("AGENTS.md");
        let mut file = std::fs::File::create(&agents_md).unwrap();
        writeln!(file, "# Agent Instructions\n\nFollow these rules...").unwrap();

        let builder = LayeredPromptBuilder::new().with_agents_md(temp_dir.path());

        assert!(builder.has_layer("agents_md"));
        let layer = builder.get_layer("agents_md").unwrap();
        assert_eq!(layer.priority, 200);
        assert!(layer.content.contains("Agent Instructions"));
    }

    #[test]
    fn test_claude_md_fallback() {
        let temp_dir = TempDir::new().unwrap();
        let claude_md = temp_dir.path().join("CLAUDE.md");
        let mut file = std::fs::File::create(&claude_md).unwrap();
        writeln!(file, "# Claude Instructions\n\nBe helpful...").unwrap();

        let builder = LayeredPromptBuilder::new().with_agents_md(temp_dir.path());

        assert!(builder.has_layer("agents_md"));
        let layer = builder.get_layer("agents_md").unwrap();
        assert_eq!(layer.priority, 200);
        assert!(layer.content.contains("Claude Instructions"));
    }

    #[test]
    fn test_agents_md_preferred_over_claude_md() {
        let temp_dir = TempDir::new().unwrap();

        // Create both files
        let agents_md = temp_dir.path().join("AGENTS.md");
        let mut file1 = std::fs::File::create(&agents_md).unwrap();
        writeln!(file1, "AGENTS.md content").unwrap();

        let claude_md = temp_dir.path().join("CLAUDE.md");
        let mut file2 = std::fs::File::create(&claude_md).unwrap();
        writeln!(file2, "CLAUDE.md content").unwrap();

        let builder = LayeredPromptBuilder::new().with_agents_md(temp_dir.path());

        // Should prefer AGENTS.md
        let layer = builder.get_layer("agents_md").unwrap();
        assert!(layer.content.contains("AGENTS.md content"));
        assert!(!layer.content.contains("CLAUDE.md content"));
    }

    #[test]
    fn test_agent_card_layer() {
        let builder = LayeredPromptBuilder::new().with_agent_card("You are a Rust expert.");

        assert!(builder.has_layer("agent_card"));
        let layer = builder.get_layer("agent_card").unwrap();
        assert_eq!(layer.priority, 300);
        assert_eq!(layer.content, "You are a Rust expert.");
    }

    #[test]
    fn test_full_stack_build() {
        let temp_dir = TempDir::new().unwrap();
        let agents_md = temp_dir.path().join("AGENTS.md");
        let mut file = std::fs::File::create(&agents_md).unwrap();
        writeln!(file, "Project guidelines").unwrap();

        let builder = LayeredPromptBuilder::new()
            .with_agents_md(temp_dir.path())
            .with_agent_card("Agent personality")
            .with_user_prompt("User preferences");

        let result = builder.build();

        // Verify all layers present
        assert!(result.contains("You are a helpful assistant")); // base
        assert!(result.contains("Project guidelines")); // agents_md
        assert!(result.contains("Agent personality")); // agent_card
        assert!(result.contains("User preferences")); // user

        // Verify ordering (base < agents_md < agent_card < user)
        let base_pos = result.find("helpful assistant").unwrap();
        let agents_pos = result.find("Project guidelines").unwrap();
        let card_pos = result.find("Agent personality").unwrap();
        let user_pos = result.find("User preferences").unwrap();

        assert!(base_pos < agents_pos);
        assert!(agents_pos < card_pos);
        assert!(card_pos < user_pos);
    }

    #[test]
    fn test_empty_layers_filtered() {
        let mut builder = LayeredPromptBuilder::new();
        builder.add_layer_internal("empty", PromptLayer::new(""));

        let result = builder.build();

        // Empty layer should be filtered out
        assert_eq!(result, "You are a helpful assistant.");
    }

    #[test]
    fn test_optional_empty_layers_filtered() {
        let mut builder = LayeredPromptBuilder::new();
        builder.add_layer_internal("optional_empty", PromptLayer::new("").optional());

        let result = builder.build();

        // Optional empty layer should be filtered out
        assert_eq!(result, "You are a helpful assistant.");
    }

    #[test]
    fn test_optional_with_content_included() {
        let mut builder = LayeredPromptBuilder::new();
        builder.add_layer_internal(
            "optional_filled",
            PromptLayer::new("Optional content")
                .optional()
                .with_priority(50),
        );

        let result = builder.build();

        // Optional layer with content should be included
        assert!(result.contains("Optional content"));
    }

    #[test]
    fn test_custom_separator() {
        let builder = LayeredPromptBuilder::new()
            .with_separator("\n\n===\n\n")
            .with_agent_card("Agent prompt");

        let result = builder.build();

        assert!(result.contains("\n\n===\n\n"));
    }

    #[test]
    fn test_builder_remove_layer() {
        let mut builder = LayeredPromptBuilder::new();
        builder.add_layer_internal("temp", PromptLayer::new("Temporary"));

        let removed = builder.remove_layer("temp");
        assert!(removed.is_some());
        assert!(!builder.has_layer("temp"));
    }

    #[test]
    fn test_builder_get_layer() {
        let mut builder = LayeredPromptBuilder::new();
        builder.add_layer_internal("test", PromptLayer::new("Test content"));

        let layer = builder.get_layer("test");
        assert!(layer.is_some());
        assert_eq!(layer.unwrap().content, "Test content");
    }

    #[test]
    fn test_builder_clear() {
        let mut builder = LayeredPromptBuilder::new();
        assert!(builder.has_layer("base")); // Should have base layer

        builder.clear();
        assert_eq!(builder.layer_names().len(), 0);
        assert!(!builder.has_layer("base"));
    }

    #[test]
    fn test_estimated_tokens() {
        let mut builder = LayeredPromptBuilder::new();
        // Clear base layer for predictable count
        builder.clear();
        builder.add_layer_internal("a", PromptLayer::new("a".repeat(400)));
        builder.add_layer_internal("b", PromptLayer::new("b".repeat(400)));

        assert_eq!(builder.estimated_tokens(), 200);
    }

    #[test]
    fn test_no_agents_md_file() {
        let temp_dir = TempDir::new().unwrap();
        let builder = LayeredPromptBuilder::new().with_agents_md(temp_dir.path());

        // Should not have agents_md layer if file doesn't exist
        assert!(!builder.has_layer("agents_md"));
    }
}
