//! Crucible Desktop - Native GPUI chat interface
//!
//! This crate provides a native desktop chat UI built on GPUI framework.
//! It follows SOLID principles by consuming traits from crucible-core.
//!
//! ## Architecture
//!
//! - **ChatBackend**: Trait for chat message handling (implemented by LLM providers)
//! - **App**: Root GPUI application state
//! - **ChatView**: Main chat interface view
//! - **MessageList**: Scrollable list of messages
//! - **ChatInput**: Multiline text input with send action

pub mod app;
pub mod backend;
pub mod chat;
pub mod theme;

// Re-export core traits we consume
pub use crucible_core::traits::chat::{
    AgentHandle, ChatChunk, ChatContext, ChatMode, ChatResponse, ChatResult, ChatToolCall,
    CommandDescriptor,
};

// Re-export backend types
pub use backend::MockAgent;

/// Message role in conversation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageRole {
    User,
    Assistant,
}

/// A chat message
#[derive(Debug, Clone)]
pub struct Message {
    pub role: MessageRole,
    pub content: String,
}

impl Message {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::User,
            content: content.into(),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: content.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_creation() {
        let user_msg = Message::user("Hello");
        assert_eq!(user_msg.role, MessageRole::User);
        assert_eq!(user_msg.content, "Hello");

        let assistant_msg = Message::assistant("Hi there!");
        assert_eq!(assistant_msg.role, MessageRole::Assistant);
        assert_eq!(assistant_msg.content, "Hi there!");
    }
}
