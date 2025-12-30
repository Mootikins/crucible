//! Prompt builder trait for layered system prompt assembly
//!
//! This module defines the abstraction for building system prompts from
//! multiple sources with priority ordering.
//!
//! ## Layer Priority Convention
//!
//! The prompt layering stacks from bottom to top:
//! 1. Base prompt (priority 100): Minimal default behavior
//! 2. AGENTS.md / CLAUDE.md (priority 200): Project-specific instructions
//! 3. Agent card (priority 300): Agent-specific persona
//! 4. User customization (priority 400): Runtime overrides
//! 5. Dynamic context (priority 500+): Session-specific injections

/// Trait for building layered system prompts
///
/// Implementations combine multiple prompt sources into a coherent
/// system prompt with clear layer separation.
pub trait PromptBuilder: Send + Sync {
    /// Add a prompt layer with a name and priority
    ///
    /// Higher priority layers appear later in the final prompt.
    /// If a layer with the same name exists, it is replaced.
    fn add_layer(&mut self, priority: u32, name: &str, content: String);

    /// Remove a layer by name
    ///
    /// Returns true if the layer existed and was removed.
    fn remove_layer(&mut self, name: &str) -> bool;

    /// Check if a layer exists
    fn has_layer(&self, name: &str) -> bool;

    /// Get a layer's content by name
    fn get_layer(&self, name: &str) -> Option<&str>;

    /// Build the final prompt from all layers
    ///
    /// Layers are ordered by priority (lowest to highest) and
    /// joined with a separator.
    fn build(&self) -> String;

    /// Estimate total token count for all layers
    fn estimated_tokens(&self) -> usize;

    /// Get all layer names
    fn layer_names(&self) -> Vec<&str>;

    /// Clear all layers
    fn clear(&mut self);
}

/// Standard layer priorities
pub mod priorities {
    /// Base prompt (minimal default)
    pub const BASE: u32 = 100;
    /// Project instructions (AGENTS.md/CLAUDE.md)
    pub const PROJECT: u32 = 200;
    /// Agent card system prompt
    pub const AGENT_CARD: u32 = 300;
    /// User customization
    pub const USER: u32 = 400;
    /// Dynamic context (KB, tools, etc.)
    pub const DYNAMIC: u32 = 500;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_priority_ordering() {
        assert!(priorities::BASE < priorities::PROJECT);
        assert!(priorities::PROJECT < priorities::AGENT_CARD);
        assert!(priorities::AGENT_CARD < priorities::USER);
        assert!(priorities::USER < priorities::DYNAMIC);
    }
}
