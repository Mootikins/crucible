//! Layered Prompt Builder
//!
//! Assembles system prompts from multiple sources with clear separation.

use crucible_core::traits::{priorities, PromptBuilder};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// A layer in the prompt hierarchy
#[derive(Debug, Clone)]
struct PromptLayer {
    /// The prompt content
    content: String,
    /// Priority for ordering (higher = later in final prompt)
    priority: u32,
}

impl PromptLayer {
    fn new(content: impl Into<String>, priority: u32) -> Self {
        Self {
            content: content.into(),
            priority,
        }
    }

    fn estimated_tokens(&self) -> usize {
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
/// 1. Base prompt (priority 100): Minimal default behavior
/// 2. AGENTS.md / CLAUDE.md (priority 200): Project-specific instructions
/// 3. Agent card system prompt (priority 300): Agent-specific persona
/// 4. User customization (priority 400): Runtime overrides
/// 5. Dynamic context (priority 500+): Session-specific injections
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
        builder.layers.insert(
            "base".to_string(),
            PromptLayer::new("You are a helpful assistant.", priorities::BASE),
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
            self.layers.insert(
                "agents_md".to_string(),
                PromptLayer::new(content, priorities::PROJECT),
            );
        } else if let Ok(content) = fs::read_to_string(&claude_path) {
            self.layers.insert(
                "agents_md".to_string(),
                PromptLayer::new(content, priorities::PROJECT),
            );
        }
        self
    }

    /// Load project rules from first matching file (Zed-compatible)
    ///
    /// Checks in order: .rules, .cursorrules, AGENTS.md, CLAUDE.md, .github/copilot-instructions.md
    /// Loads first match only (deduplication via first-match-wins)
    pub fn with_project_rules(mut self, dir: &Path) -> Self {
        let candidates = [
            ".rules",
            ".cursorrules",
            "AGENTS.md",
            "CLAUDE.md",
            ".github/copilot-instructions.md",
        ];

        for candidate in candidates {
            let path = dir.join(candidate);
            if let Ok(content) = fs::read_to_string(&path) {
                if !content.trim().is_empty() {
                    self.layers.insert(
                        "project_rules".to_string(),
                        PromptLayer::new(content, priorities::PROJECT),
                    );
                    break; // First match wins
                }
            }
        }
        self
    }

    /// Add agent card system prompt
    pub fn with_agent_card(mut self, system_prompt: impl Into<String>) -> Self {
        self.layers.insert(
            "agent_card".to_string(),
            PromptLayer::new(system_prompt, priorities::AGENT_CARD),
        );
        self
    }

    /// Add user customization prompt
    pub fn with_user_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.layers.insert(
            "user".to_string(),
            PromptLayer::new(prompt, priorities::USER),
        );
        self
    }

    /// Get the separator used between layers
    pub fn separator(&self) -> &str {
        &self.separator
    }
}

impl PromptBuilder for LayeredPromptBuilder {
    fn add_layer(&mut self, priority: u32, name: &str, content: String) {
        self.layers
            .insert(name.to_string(), PromptLayer::new(content, priority));
    }

    fn remove_layer(&mut self, name: &str) -> bool {
        self.layers.remove(name).is_some()
    }

    fn has_layer(&self, name: &str) -> bool {
        self.layers.contains_key(name)
    }

    fn get_layer(&self, name: &str) -> Option<&str> {
        self.layers.get(name).map(|l| l.content.as_str())
    }

    fn build(&self) -> String {
        let mut layers: Vec<_> = self.layers.values().collect();
        layers.sort_by_key(|l| l.priority);

        layers
            .iter()
            .filter(|l| !l.content.is_empty())
            .map(|l| l.content.as_str())
            .collect::<Vec<_>>()
            .join(&self.separator)
    }

    fn estimated_tokens(&self) -> usize {
        self.layers.values().map(|l| l.estimated_tokens()).sum()
    }

    fn layer_names(&self) -> Vec<&str> {
        self.layers.keys().map(|s| s.as_str()).collect()
    }

    fn clear(&mut self) {
        self.layers.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_default_has_base_prompt() {
        let builder = LayeredPromptBuilder::new();
        assert!(builder.has_layer("base"));
        let base = builder.get_layer("base").unwrap();
        assert_eq!(base, "You are a helpful assistant.");
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
        assert!(layer.contains("Agent Instructions"));
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
        assert!(layer.contains("Claude Instructions"));
    }

    #[test]
    fn test_add_and_remove_layer() {
        let mut builder = LayeredPromptBuilder::new();

        builder.add_layer(150, "custom", "Custom content".to_string());
        assert!(builder.has_layer("custom"));

        let removed = builder.remove_layer("custom");
        assert!(removed);
        assert!(!builder.has_layer("custom"));
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
    fn test_clear() {
        let mut builder = LayeredPromptBuilder::new();
        assert!(builder.has_layer("base"));

        builder.clear();
        assert!(!builder.has_layer("base"));
        assert!(builder.layer_names().is_empty());
    }

    #[test]
    fn test_estimated_tokens() {
        let mut builder = LayeredPromptBuilder::new();
        builder.clear();
        builder.add_layer(100, "a", "a".repeat(400));
        builder.add_layer(200, "b", "b".repeat(400));

        assert_eq!(builder.estimated_tokens(), 200);
    }

    #[test]
    fn test_rules_file_first_match_wins() {
        let temp_dir = TempDir::new().unwrap();

        // Create both .rules and AGENTS.md
        let rules = temp_dir.path().join(".rules");
        let agents = temp_dir.path().join("AGENTS.md");

        std::fs::write(&rules, "Rules content").unwrap();
        std::fs::write(&agents, "Agents content").unwrap();

        let builder = LayeredPromptBuilder::new().with_project_rules(temp_dir.path());

        // .rules should win
        let layer = builder.get_layer("project_rules").unwrap();
        assert!(layer.contains("Rules content"));
        assert!(!layer.contains("Agents content"));
    }

    #[test]
    fn test_rules_file_fallback_to_agents_md() {
        let temp_dir = TempDir::new().unwrap();

        // Create only AGENTS.md (no .rules or .cursorrules)
        let agents = temp_dir.path().join("AGENTS.md");
        std::fs::write(&agents, "Agents content").unwrap();

        let builder = LayeredPromptBuilder::new().with_project_rules(temp_dir.path());

        // AGENTS.md should be loaded
        let layer = builder.get_layer("project_rules").unwrap();
        assert!(layer.contains("Agents content"));
    }

    #[test]
    fn test_rules_file_skips_empty_files() {
        let temp_dir = TempDir::new().unwrap();

        // Create empty .rules and non-empty AGENTS.md
        let rules = temp_dir.path().join(".rules");
        let agents = temp_dir.path().join("AGENTS.md");

        std::fs::write(&rules, "   \n  ").unwrap(); // Whitespace only
        std::fs::write(&agents, "Agents content").unwrap();

        let builder = LayeredPromptBuilder::new().with_project_rules(temp_dir.path());

        // Should skip empty .rules and use AGENTS.md
        let layer = builder.get_layer("project_rules").unwrap();
        assert!(layer.contains("Agents content"));
    }

    #[test]
    fn test_rules_file_copilot_instructions() {
        let temp_dir = TempDir::new().unwrap();

        // Create .github/copilot-instructions.md
        let github_dir = temp_dir.path().join(".github");
        std::fs::create_dir(&github_dir).unwrap();
        let copilot = github_dir.join("copilot-instructions.md");
        std::fs::write(&copilot, "Copilot instructions").unwrap();

        let builder = LayeredPromptBuilder::new().with_project_rules(temp_dir.path());

        // Should load copilot-instructions.md
        let layer = builder.get_layer("project_rules").unwrap();
        assert!(layer.contains("Copilot instructions"));
    }

    #[test]
    fn test_rules_file_no_match() {
        let temp_dir = TempDir::new().unwrap();

        // No rules files exist
        let builder = LayeredPromptBuilder::new().with_project_rules(temp_dir.path());

        // No project_rules layer should exist
        assert!(!builder.has_layer("project_rules"));
    }
}
