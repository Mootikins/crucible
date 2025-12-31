//! Size-based system prompt templates

use super::ModelSize;

/// Base system prompt for small models (< 4B)
///
/// Explicit guidance to avoid tool loops
pub const SMALL_MODEL_PROMPT: &str = r#"You are a helpful assistant.

## When to Use Tools
- ONLY use tools when the user explicitly asks for file operations, searches, or commands
- For questions, math, JSON, formatting: respond directly WITHOUT tools
- Do NOT call tools for: definitions, explanations, code generation, data formatting

## Available Tools (use sparingly)
- read_file - Read file contents
- glob - Find files by pattern
- grep - Search file contents

When in doubt, respond directly without using tools."#;

/// Base system prompt for medium models (4-30B)
pub const MEDIUM_MODEL_PROMPT: &str = r#"You are a helpful assistant with access to workspace tools.

## Tool Usage
- Use tools when tasks require file operations or system interaction
- For simple questions and formatting: respond directly
- Available: read_file, write_file, edit_file, bash, glob, grep

Do NOT output XML-style tool tags - use native function calling format."#;

/// Base system prompt for large models (> 30B)
pub const LARGE_MODEL_PROMPT: &str = "You are a helpful assistant with workspace tools available.";

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
    fn test_small_prompt_has_explicit_guidance() {
        let prompt = base_prompt_for_size(ModelSize::Small);
        assert!(prompt.contains("ONLY use tools"));
        assert!(prompt.contains("Do NOT call tools"));
    }

    #[test]
    fn test_medium_prompt_lists_all_tools() {
        let prompt = base_prompt_for_size(ModelSize::Medium);
        assert!(prompt.contains("write_file"));
        assert!(prompt.contains("edit_file"));
        assert!(prompt.contains("bash"));
    }

    #[test]
    fn test_large_prompt_is_minimal() {
        let prompt = base_prompt_for_size(ModelSize::Large);
        assert!(prompt.len() < 100);
    }
}
