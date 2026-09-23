//! Response streaming for ACP sessions
//!
//! This module handles streaming responses from agents, including message chunks,
//! tool calls, and thought processes.
//!
//! ## Design Principles
//!
//! - **Single Responsibility**: Focused on streaming and formatting responses
//! - **Open/Closed**: Extensible for different output formats
//! - **Dependency Inversion**: Uses core types, protocol-agnostic

/// What one turn showed the user, for the stop reason and the batch end.
///
/// The client counts this from the same events it sends to the turn stream,
/// so the consumer does not count them a second time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TurnSummary {
    /// The turn produced text or a thought with a visible character.
    pub produced_content: bool,
    /// The turn announced at least one tool call.
    pub announced_any: bool,
}

/// The token usage that the agent reported at the end of a turn.
///
/// ACP puts it on `PromptResponse.usage`, behind the
/// `unstable_end_turn_token_usage` schema feature. An agent that sends a
/// partial or snake_case record reports no usage: the schema drops it.
pub fn turn_usage(
    response: &agent_client_protocol::schema::v1::PromptResponse,
) -> Option<crucible_core::traits::llm::TokenUsage> {
    let usage = response.usage.as_ref()?;
    let tokens = |n: u64| u32::try_from(n).unwrap_or(u32::MAX);
    Some(crucible_core::traits::llm::TokenUsage {
        prompt_tokens: tokens(usage.input_tokens),
        completion_tokens: tokens(usage.output_tokens),
        total_tokens: tokens(usage.total_tokens),
        cache_read_tokens: usage.cached_read_tokens.map(tokens),
        cache_creation_tokens: usage.cached_write_tokens.map(tokens),
    })
}

/// Convert a tool title into a human-readable name by removing MCP schema prefixes
/// and title-casing the result.
///
/// Handles patterns:
/// - `mcp__crucible__semantic_search` → `Semantic Search`
/// - `mcp__create_issue` → `Create Issue`
/// - `mcp_write` → `Write`
/// - `plugin_NAME_NAME__search` → `Search`
/// - `Read File` → `Read File` (already clean)
pub fn humanize_tool_title(title: &str) -> String {
    // Strip known prefixes
    let stripped = if let Some(s) = title.strip_prefix("mcp__crucible__") {
        s
    } else if let Some(s) = title.strip_prefix("mcp__") {
        s
    } else if let Some(s) = title.strip_prefix("mcp_") {
        s
    } else if let Some(s) = title.strip_prefix("plugin_") {
        // plugin_NAME_NAME__X → take X (part after __)
        if let Some(after_double_underscore) = s.split("__").last() {
            after_double_underscore
        } else {
            s
        }
    } else {
        title
    };

    // Title-case: convert snake_case to Title Case
    title_case(stripped)
}

/// Convert snake_case or kebab-case to Title Case.
/// Examples: `semantic_search` → `Semantic Search`, `create-issue` → `Create Issue`
/// If the input contains no alphanumeric characters, returns it unchanged.
fn title_case(s: &str) -> String {
    let words: Vec<String> = s
        .split(['_', '-'])
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect();

    if words.is_empty() {
        s.to_string()
    } else {
        words.join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn humanize_tool_title_mcp_double_underscore_crucible() {
        assert_eq!(
            humanize_tool_title("mcp__crucible__semantic_search"),
            "Semantic Search"
        );
    }

    #[test]
    fn humanize_tool_title_mcp_double_underscore() {
        assert_eq!(humanize_tool_title("mcp__create_issue"), "Create Issue");
    }

    #[test]
    fn humanize_tool_title_mcp_single_underscore() {
        assert_eq!(humanize_tool_title("mcp_write"), "Write");
    }

    #[test]
    fn humanize_tool_title_plugin_prefix() {
        assert_eq!(
            humanize_tool_title("plugin_episodic-memory_episodic-memory__search"),
            "Search"
        );
    }

    #[test]
    fn humanize_tool_title_already_clean() {
        assert_eq!(humanize_tool_title("Read File"), "Read File");
    }

    #[test]
    fn humanize_tool_title_simple_snake_case() {
        assert_eq!(humanize_tool_title("search"), "Search");
    }

    #[test]
    fn humanize_tool_title_kebab_case() {
        assert_eq!(humanize_tool_title("create-issue"), "Create Issue");
    }

    #[test]
    fn humanize_tool_title_complex_snake_case() {
        assert_eq!(
            humanize_tool_title("list_all_files_recursively"),
            "List All Files Recursively"
        );
    }
}
