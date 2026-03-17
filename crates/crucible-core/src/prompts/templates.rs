//! Default system prompt template

/// Minimal fallback system prompt — used when Lua init.lua hasn't set one.
/// The real default is in `crates/crucible-lua/src/defaults/init.lua`.
pub const DEFAULT_SYSTEM_PROMPT: &str = "Answer from the notes and context provided to you. If information isn't in your context, say so — do not fabricate. Be brief.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_prompt_has_grounding() {
        assert!(DEFAULT_SYSTEM_PROMPT.contains("do not fabricate"));
    }

    #[test]
    fn test_default_prompt_has_brevity() {
        assert!(DEFAULT_SYSTEM_PROMPT.contains("Be brief"));
    }

    #[test]
    fn test_default_prompt_no_product_name() {
        assert!(!DEFAULT_SYSTEM_PROMPT.contains("Crucible"));
        assert!(!DEFAULT_SYSTEM_PROMPT.contains("helpful assistant"));
    }
}
