//! Session/conversation fixtures
//!
//! Pre-built conversation histories for testing various scenarios.

use crate::tui::content_block::ContentBlock;
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
        blocks: vec![ContentBlock::prose(text.into())],
        is_streaming: false,
    }
}

/// Helper to create an assistant message with multiple blocks
pub fn assistant_blocks(blocks: Vec<ContentBlock>) -> ConversationItem {
    ConversationItem::AssistantMessage {
        blocks,
        is_streaming: false,
    }
}

/// Helper to create a tool call
pub fn tool_call(name: impl Into<String>, status: ToolStatus) -> ConversationItem {
    ConversationItem::ToolCall(ToolCallDisplay {
        name: name.into(),
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
            ContentBlock::prose("Here's an example:"),
            ContentBlock::code(
                Some("rust".to_string()),
                "fn main() {\n    println!(\"Hello\");\n}",
            ),
            ContentBlock::prose("This prints Hello."),
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
        assert!(matches!(session[1], ConversationItem::AssistantMessage { .. }));
    }
}
