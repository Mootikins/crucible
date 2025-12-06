//! Mock agent for testing
//!
//! Provides a simple mock implementation of AgentHandle for development and testing.

use async_trait::async_trait;

use crate::{AgentHandle, ChatMode, ChatResponse, ChatResult, CommandDescriptor};

/// Mock agent that provides canned responses
///
/// Used for development and testing of the chat UI.
pub struct MockAgent {
    mode: ChatMode,
    connected: bool,
}

impl MockAgent {
    /// Create a new mock agent
    pub fn new() -> Self {
        Self {
            mode: ChatMode::Plan,
            connected: true,
        }
    }

    /// Synchronous message handler for simple UI integration
    ///
    /// Use this for initial development; the async version should be used
    /// for real agent integration.
    pub fn send_message_sync(&self, message: &str) -> String {
        generate_mock_response(message)
    }
}

impl Default for MockAgent {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AgentHandle for MockAgent {
    async fn send_message(&mut self, message: &str) -> ChatResult<ChatResponse> {
        // Generate a mock response based on the input
        let content = generate_mock_response(message);

        Ok(ChatResponse {
            content,
            tool_calls: Vec::new(),
        })
    }

    async fn set_mode(&mut self, mode: ChatMode) -> ChatResult<()> {
        self.mode = mode;
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.connected
    }

    fn supports_streaming(&self) -> bool {
        false
    }

    async fn on_commands_update(&mut self, _commands: Vec<CommandDescriptor>) -> ChatResult<()> {
        Ok(())
    }
}

/// Generate a mock response based on the input message
fn generate_mock_response(message: &str) -> String {
    let msg_lower = message.to_lowercase();

    if msg_lower.contains("hello") || msg_lower.contains("hi") {
        return "Hello! I'm a mock Crucible assistant. I can help you explore and manage your knowledge base. Try asking me about:\n\n- **Searching** for notes\n- **Viewing** recent changes\n- **Understanding** how Crucible works".to_string();
    }

    if msg_lower.contains("search") {
        return "I can help you search your knowledge base! In the real implementation, I would:\n\n1. Use **semantic search** to find related notes\n2. Show **relevance scores** for each result\n3. Let you **navigate** directly to matching content\n\n*This is a mock response - real search integration coming soon!*".to_string();
    }

    if msg_lower.contains("help") {
        return "## Crucible Chat Help\n\nHere are some things I can help with:\n\n- `/search <query>` - Search your knowledge base\n- `/plan` - Enter planning mode (read-only)\n- `/act` - Enter action mode (can make changes)\n- `/exit` - Exit the chat\n\n**Keyboard shortcuts:**\n- `Cmd+Enter` - Send message\n- `Cmd+K` - Clear chat\n- `Escape` - Cancel".to_string();
    }

    if msg_lower.contains("markdown") || msg_lower.contains("test") {
        return "Here's a **markdown** test:\n\n## Headers work\n\nAnd so do:\n- Bullet points\n- **Bold text**\n- *Italic text*\n- `inline code`\n\n```rust\nfn hello() {\n    println!(\"Hello, world!\");\n}\n```\n\n> Blockquotes too!".to_string();
    }

    // Default response
    format!(
        "I received your message: *\"{}\"*\n\n\
        This is a mock response. In the full implementation, I would:\n\n\
        1. Process your request using the Crucible agent framework\n\
        2. Access your knowledge base if needed\n\
        3. Provide helpful, contextual responses\n\n\
        *Mock agent for development purposes.*",
        message
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_agent_new() {
        let agent = MockAgent::new();
        assert!(agent.is_connected());
    }

    #[tokio::test]
    async fn test_mock_agent_send_message() {
        let mut agent = MockAgent::new();
        let response = agent.send_message("hello").await.unwrap();
        assert!(response.content.contains("Hello"));
        assert!(response.tool_calls.is_empty());
    }

    #[tokio::test]
    async fn test_mock_agent_set_mode() {
        let mut agent = MockAgent::new();
        agent.set_mode(ChatMode::Act).await.unwrap();
        // Mode is stored internally
    }

    #[tokio::test]
    async fn test_mock_agent_help_response() {
        let mut agent = MockAgent::new();
        let response = agent.send_message("help me").await.unwrap();
        assert!(response.content.contains("Help"));
    }

    #[tokio::test]
    async fn test_mock_agent_markdown_response() {
        let mut agent = MockAgent::new();
        let response = agent.send_message("test markdown").await.unwrap();
        assert!(response.content.contains("**markdown**"));
        assert!(response.content.contains("```rust"));
    }
}
