//! Session/conversation fixtures
//!
//! Pre-built conversation histories for testing various scenarios.

use crate::tui::content_block::StreamBlock;
use crate::tui::conversation::{ConversationItem, ToolCallDisplay, ToolStatus};

/// Helper to create a user message
pub fn user(text: impl Into<String>) -> ConversationItem {
    ConversationItem::UserMessage {
        content: text.into(),
    }
}

/// Helper to create an assistant message
pub fn assistant(text: impl Into<String>) -> ConversationItem {
    ConversationItem::AssistantMessage {
        blocks: vec![StreamBlock::prose(text.into())],
        is_streaming: false,
    }
}

/// Helper to create an assistant message with multiple blocks
pub fn assistant_blocks(blocks: Vec<StreamBlock>) -> ConversationItem {
    ConversationItem::AssistantMessage {
        blocks,
        is_streaming: false,
    }
}

/// Helper to create a tool call
pub fn tool_call(name: impl Into<String>, status: ToolStatus) -> ConversationItem {
    ConversationItem::ToolCall(ToolCallDisplay {
        name: name.into(),
        args: serde_json::json!({}),
        status,
        output_lines: vec![],
    })
}

/// Helper to create a completed tool call
pub fn tool_complete(name: impl Into<String>) -> ConversationItem {
    tool_call(name, ToolStatus::Complete { summary: None })
}

/// Helper to create a running tool call
pub fn tool_running(name: impl Into<String>) -> ConversationItem {
    tool_call(name, ToolStatus::Running)
}

/// Helper to create a completed tool call with summary
pub fn tool_complete_with_summary(
    name: impl Into<String>,
    summary: impl Into<String>,
) -> ConversationItem {
    tool_call(
        name,
        ToolStatus::Complete {
            summary: Some(summary.into()),
        },
    )
}

/// Helper to create an errored tool call
pub fn tool_error(name: impl Into<String>, message: impl Into<String>) -> ConversationItem {
    tool_call(
        name,
        ToolStatus::Error {
            message: message.into(),
        },
    )
}

/// Helper to create a tool call with output lines
pub fn tool_with_output(
    name: impl Into<String>,
    status: ToolStatus,
    output: Vec<&str>,
) -> ConversationItem {
    ConversationItem::ToolCall(ToolCallDisplay {
        name: name.into(),
        args: serde_json::json!({}),
        status,
        output_lines: output.into_iter().map(|s| s.to_string()).collect(),
    })
}

/// Empty session, fresh start
pub fn empty() -> Vec<ConversationItem> {
    vec![]
}

/// Simple back-and-forth
pub fn basic_exchange() -> Vec<ConversationItem> {
    vec![user("Hello"), assistant("Hi! How can I help?")]
}

/// Multi-turn with context
pub fn multi_turn() -> Vec<ConversationItem> {
    vec![
        user("What is Crucible?"),
        assistant("Crucible is a knowledge management system that combines wikilinks with AI."),
        user("How do I search?"),
        assistant("Use the `/search` command or `@` to reference files."),
    ]
}

/// Session with tool calls
pub fn with_tool_calls() -> Vec<ConversationItem> {
    vec![
        user("Read my config"),
        assistant("I'll read that for you."),
        tool_complete("read_file"),
        assistant("Your config contains a key-value pair."),
    ]
}

/// Long session for scroll testing (50 messages)
pub fn long_conversation() -> Vec<ConversationItem> {
    (0..50)
        .map(|i| {
            if i % 2 == 0 {
                user(format!("Message {i}"))
            } else {
                assistant(format!("Response {i}"))
            }
        })
        .collect()
}

/// Session with multiline content
pub fn multiline_messages() -> Vec<ConversationItem> {
    vec![
        user("Show me some code"),
        assistant_blocks(vec![
            StreamBlock::prose("Here's an example:"),
            StreamBlock::code(
                Some("rust".to_string()),
                "fn main() {\n    println!(\"Hello\");\n}",
            ),
            StreamBlock::prose("This prints Hello."),
        ]),
    ]
}

// =============================================================================
// Tool Call Scenarios
// =============================================================================

/// Single running tool call
pub fn tool_call_running() -> Vec<ConversationItem> {
    vec![
        user("Search the codebase"),
        assistant("I'll search for that."),
        tool_running("grep"),
    ]
}

/// Single complete tool call with summary
pub fn tool_call_complete() -> Vec<ConversationItem> {
    vec![
        user("List files"),
        assistant("Let me check."),
        tool_complete_with_summary("glob", "42 files"),
    ]
}

/// Tool call that errored
pub fn tool_call_error() -> Vec<ConversationItem> {
    vec![
        user("Read secret file"),
        assistant("I'll try to read that."),
        tool_error("read_file", "Permission denied"),
    ]
}

/// Tool with streaming output
pub fn tool_call_with_output() -> Vec<ConversationItem> {
    vec![
        user("Find TODO comments"),
        tool_with_output(
            "grep",
            ToolStatus::Running,
            vec![
                "src/main.rs:42: TODO: fix this",
                "src/lib.rs:17: TODO: optimize",
            ],
        ),
    ]
}

/// Multiple sequential tool calls (common pattern)
pub fn multiple_tool_calls() -> Vec<ConversationItem> {
    vec![
        user("Find and read the config"),
        assistant("I'll search for config files first."),
        tool_complete_with_summary("glob", "3 files"),
        tool_complete_with_summary("read_file", "127 lines"),
        assistant("Found the config with database settings."),
    ]
}

/// Interleaved prose and tool calls (complex streaming scenario)
pub fn interleaved_prose_and_tools() -> Vec<ConversationItem> {
    vec![
        user("Analyze this codebase"),
        assistant("Let me explore the structure."),
        tool_complete_with_summary("glob", "src/**/*.rs - 24 files"),
        assistant("I found 24 Rust files. Let me check the main entry point."),
        tool_complete_with_summary("read_file", "src/main.rs"),
        assistant("The main.rs initializes the CLI. Now checking dependencies."),
        tool_complete_with_summary("read_file", "Cargo.toml"),
        assistant("This project uses tokio for async runtime and clap for CLI parsing."),
    ]
}

/// Multiple tool calls in a single agent turn (batched, no prose between)
/// This is common when an LLM decides to call multiple tools at once
pub fn batched_tool_calls() -> Vec<ConversationItem> {
    vec![
        user("Find all Rust files and their sizes"),
        assistant("I'll search for Rust files and check their sizes."),
        // These three tools are called in a single agent turn (batch)
        tool_complete_with_summary("glob", "src/**/*.rs - 24 files"),
        tool_complete_with_summary("glob", "tests/**/*.rs - 12 files"),
        tool_complete_with_summary("glob", "examples/**/*.rs - 5 files"),
        assistant("Found 41 total Rust files across src, tests, and examples."),
    ]
}

/// Mixed success and error tool calls
pub fn mixed_tool_results() -> Vec<ConversationItem> {
    vec![
        user("Read all config files"),
        assistant("I'll read each config file."),
        tool_complete_with_summary("read_file", "config.toml - 45 lines"),
        tool_error("read_file", ".env: File not found"),
        tool_complete_with_summary("read_file", "settings.json - 12 lines"),
        assistant("Read 2 of 3 config files. The .env file doesn't exist."),
    ]
}

// =============================================================================
// Code Block Scenarios
// =============================================================================

/// Session with a Rust code block
pub fn with_rust_code() -> Vec<ConversationItem> {
    vec![
        user("Show me Rust"),
        assistant_blocks(vec![
            StreamBlock::prose("Here's an example:"),
            StreamBlock::code(
                Some("rust".to_string()),
                "fn main() {\n    println!(\"Hello!\");\n}",
            ),
        ]),
    ]
}

/// Session with multiple code blocks in different languages
pub fn with_multi_lang_code() -> Vec<ConversationItem> {
    vec![
        user("Compare languages"),
        assistant_blocks(vec![
            StreamBlock::prose("Python:"),
            StreamBlock::code(Some("python".to_string()), "print(\"Hello\")"),
            StreamBlock::prose("Rust:"),
            StreamBlock::code(Some("rust".to_string()), "println!(\"Hello\");"),
        ]),
    ]
}

/// Session with a long code block (20 lines)
pub fn with_long_code() -> Vec<ConversationItem> {
    let lines: String = (1..=20)
        .map(|i| format!("    field_{}: i32,\n", i))
        .collect();
    vec![
        user("Generate struct"),
        assistant_blocks(vec![StreamBlock::code(
            Some("rust".to_string()),
            format!("struct Data {{\n{}}}", lines),
        )]),
    ]
}

/// Session with code block without language tag
pub fn with_untagged_code() -> Vec<ConversationItem> {
    vec![
        user("Show output"),
        assistant_blocks(vec![
            StreamBlock::prose("Here's the output:"),
            StreamBlock::code(None, "Some plain output\nline 2\nline 3"),
        ]),
    ]
}

/// Session with inline code in prose (backticks)
pub fn with_inline_code() -> Vec<ConversationItem> {
    vec![
        user("What is println?"),
        assistant("The `println!` macro prints to stdout. Use it like `println!(\"Hello\")`"),
    ]
}

// =============================================================================
// Table Scenarios
// =============================================================================

/// Session with a simple markdown table in assistant response
pub fn with_table() -> Vec<ConversationItem> {
    vec![
        user("Show me a comparison"),
        assistant(
            "Here's a comparison:\n\n\
             | Feature | Rust | Go |\n\
             |---------|------|----|\n\
             | Memory | Safe | GC |\n\
             | Speed | Fast | Fast |",
        ),
    ]
}

/// Session with a wider table that may need truncation
pub fn with_wide_table() -> Vec<ConversationItem> {
    vec![
        user("Show packages"),
        assistant(
            "Here are the packages:\n\n\
             | Package | Description | Version | License |\n\
             |---------|-------------|---------|----------|\n\
             | serde | Serialization framework | 1.0.195 | MIT/Apache-2.0 |\n\
             | tokio | Async runtime for Rust | 1.35.1 | MIT |\n\
             | clap | Command line parser | 4.4.12 | MIT/Apache-2.0 |",
        ),
    ]
}

/// Session with multiple tables in one response
pub fn with_multiple_tables() -> Vec<ConversationItem> {
    vec![
        user("Compare languages and their features"),
        assistant(
            "Here are two comparisons:\n\n\
             **Type Systems:**\n\n\
             | Language | Type System |\n\
             |----------|-------------|\n\
             | Rust | Static |\n\
             | Python | Dynamic |\n\n\
             **Concurrency:**\n\n\
             | Language | Model |\n\
             |----------|-------|\n\
             | Rust | Ownership |\n\
             | Go | CSP |",
        ),
    ]
}

/// Session with a table mixed with prose and code
pub fn with_table_and_code() -> Vec<ConversationItem> {
    vec![
        user("Explain the struct and show options"),
        assistant_blocks(vec![
            StreamBlock::prose("The struct has these fields:\n\n| Field | Type | Default |\n|-------|------|--------|\n| name | String | \"\" |\n| count | u32 | 0 |\n\nHere's how to use it:"),
            StreamBlock::code(
                Some("rust".to_string()),
                "let item = Item {\n    name: \"test\".into(),\n    count: 42,\n};",
            ),
            StreamBlock::prose("This creates an Item with custom values."),
        ]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixtures_return_expected_lengths() {
        assert_eq!(empty().len(), 0);
        assert_eq!(basic_exchange().len(), 2);
        assert_eq!(multi_turn().len(), 4);
        assert_eq!(with_tool_calls().len(), 4);
        assert_eq!(long_conversation().len(), 50);
    }

    #[test]
    fn basic_exchange_has_correct_roles() {
        let session = basic_exchange();
        assert!(matches!(session[0], ConversationItem::UserMessage { .. }));
        assert!(matches!(
            session[1],
            ConversationItem::AssistantMessage { .. }
        ));
    }

    #[test]
    fn tool_call_fixtures_have_correct_status() {
        let running = tool_call_running();
        assert!(matches!(
            &running[2],
            ConversationItem::ToolCall(t) if matches!(t.status, ToolStatus::Running)
        ));

        let complete = tool_call_complete();
        assert!(matches!(
            &complete[2],
            ConversationItem::ToolCall(t) if matches!(t.status, ToolStatus::Complete { .. })
        ));

        let error = tool_call_error();
        assert!(matches!(
            &error[2],
            ConversationItem::ToolCall(t) if matches!(t.status, ToolStatus::Error { .. })
        ));
    }

    #[test]
    fn tool_with_output_has_lines() {
        let session = tool_call_with_output();
        if let ConversationItem::ToolCall(tool) = &session[1] {
            assert_eq!(tool.output_lines.len(), 2);
            assert!(tool.output_lines[0].contains("TODO"));
        } else {
            panic!("Expected tool call");
        }
    }

    #[test]
    fn interleaved_alternates_correctly() {
        let session = interleaved_prose_and_tools();
        // Pattern: user, assistant, tool, assistant, tool, assistant, tool, assistant
        assert!(matches!(session[0], ConversationItem::UserMessage { .. }));
        assert!(matches!(
            session[1],
            ConversationItem::AssistantMessage { .. }
        ));
        assert!(matches!(session[2], ConversationItem::ToolCall(_)));
        assert!(matches!(
            session[3],
            ConversationItem::AssistantMessage { .. }
        ));
    }
}
