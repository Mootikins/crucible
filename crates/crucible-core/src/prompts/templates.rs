//! Size-based system prompt templates

use super::ModelSize;

/// Base system prompt for small models (< 4B)
///
/// Minimal prompt - tools are provided via function calling schema
pub const SMALL_MODEL_PROMPT: &str = r#"You are a helpful assistant.

Only use tools when the task requires file/system operations.
For questions and formatting: respond directly without tools."#;

/// Base system prompt for medium models (4-30B)
///
/// Minimal prompt - tools are provided via function calling schema
pub const MEDIUM_MODEL_PROMPT: &str = r#"You are a helpful assistant.

Use tools for file operations and system tasks. Respond directly for questions and formatting."#;

/// Base system prompt for large models (> 30B)
///
/// Minimal prompt - large models need no tool guidance
pub const LARGE_MODEL_PROMPT: &str = "You are a helpful assistant.";

/// Get the appropriate base prompt for a model size
pub fn base_prompt_for_size(size: ModelSize) -> &'static str {
    match size {
        ModelSize::Small => SMALL_MODEL_PROMPT,
        ModelSize::Medium => MEDIUM_MODEL_PROMPT,
        ModelSize::Large => LARGE_MODEL_PROMPT,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_small_prompt_has_tool_guidance() {
        let prompt = base_prompt_for_size(ModelSize::Small);
        // Small models get guidance about when to use tools vs respond directly
        assert!(prompt.contains("tools"));
        assert!(prompt.contains("respond directly"));
    }

    #[test]
    fn test_medium_prompt_is_concise() {
        let prompt = base_prompt_for_size(ModelSize::Medium);
        // Medium models get brief guidance, no tool listings
        assert!(prompt.contains("tools"));
        assert!(!prompt.contains("write_file")); // No tool names in prompt
        assert!(prompt.len() < 200);
    }

    #[test]
    fn test_large_prompt_is_minimal() {
        let prompt = base_prompt_for_size(ModelSize::Large);
        // Large models need no tool guidance - they get schemas via API
        assert!(prompt.len() < 100);
        assert!(!prompt.contains("tools")); // Truly minimal
    }
}
