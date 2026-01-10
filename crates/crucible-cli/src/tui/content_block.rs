//! Stream block types for structured message rendering
//!
//! [`StreamBlock`] represents parsed content during streaming (prose or code).
//! This is distinct from [`crate::tui::viewport::ViewportBlock`] (buffered UI blocks)
//! and [`crucible_core::traits::mcp::ContentBlock`] (MCP protocol content).

/// Events emitted by the streaming parser
#[derive(Debug, Clone, PartialEq)]
pub enum ParseEvent {
    /// Plain text content
    Text(String),
    /// Start of a code block
    CodeBlockStart { lang: Option<String> },
    /// Content within a code block
    CodeBlockContent(String),
    /// End of a code block
    CodeBlockEnd,
}

/// A structured content block within a streaming message.
///
/// Represents parsed content (prose or code) during streaming, with `is_complete`
/// flag to track whether the block is still being received.
///
/// This is distinct from:
/// - [`crate::tui::viewport::ViewportBlock`] - buffered UI blocks with IDs and caching
/// - [`crucible_core::traits::mcp::ContentBlock`] - MCP protocol content (Text, Image, Resource)
#[derive(Debug, Clone)]
pub enum StreamBlock {
    /// Markdown prose (may be partial during streaming)
    Prose { text: String, is_complete: bool },
    /// Code block with optional language
    Code {
        lang: Option<String>,
        content: String,
        is_complete: bool,
    },
    /// Tool call embedded in the assistant message stream
    Tool {
        name: String,
        args: serde_json::Value,
        status: ToolBlockStatus,
    },
}

/// Status of a tool block within a streaming message
#[derive(Debug, Clone, PartialEq)]
pub enum ToolBlockStatus {
    Running,
    Complete { summary: Option<String> },
    Error { message: String },
}

impl StreamBlock {
    pub fn prose(text: impl Into<String>) -> Self {
        Self::Prose {
            text: text.into(),
            is_complete: true,
        }
    }

    pub fn prose_partial(text: impl Into<String>) -> Self {
        Self::Prose {
            text: text.into(),
            is_complete: false,
        }
    }

    pub fn code(lang: Option<String>, content: impl Into<String>) -> Self {
        Self::Code {
            lang,
            content: content.into(),
            is_complete: true,
        }
    }

    pub fn code_partial(lang: Option<String>, content: impl Into<String>) -> Self {
        Self::Code {
            lang,
            content: content.into(),
            is_complete: false,
        }
    }

    pub fn is_complete(&self) -> bool {
        match self {
            Self::Prose { is_complete, .. } => *is_complete,
            Self::Code { is_complete, .. } => *is_complete,
            Self::Tool { status, .. } => !matches!(status, ToolBlockStatus::Running),
        }
    }

    /// Check if this is a prose block
    pub fn is_prose(&self) -> bool {
        matches!(self, Self::Prose { .. })
    }

    /// Check if this is a code block
    pub fn is_code(&self) -> bool {
        matches!(self, Self::Code { .. })
    }

    /// Check if this is a tool block
    pub fn is_tool(&self) -> bool {
        matches!(self, Self::Tool { .. })
    }

    /// Mark the block as complete
    pub fn complete(&mut self) {
        match self {
            Self::Prose { is_complete, .. } => *is_complete = true,
            Self::Code { is_complete, .. } => *is_complete = true,
            Self::Tool { .. } => {} // Tools are completed via status update
        }
    }

    /// Get the text content of the block
    pub fn text(&self) -> &str {
        match self {
            Self::Prose { text, .. } => text,
            Self::Code { content, .. } => content,
            Self::Tool { name, .. } => name,
        }
    }

    /// Get the language (only for Code blocks)
    pub fn lang(&self) -> Option<&str> {
        match self {
            Self::Code { lang, .. } => lang.as_deref(),
            _ => None,
        }
    }

    /// Append text to the block (for streaming)
    pub fn append(&mut self, text: &str) {
        match self {
            Self::Prose {
                text: ref mut t, ..
            } => t.push_str(text),
            Self::Code { content, .. } => content.push_str(text),
            Self::Tool { .. } => {} // Tools don't support text append
        }
    }

    /// Create a new tool block in running state
    pub fn tool(name: impl Into<String>, args: serde_json::Value) -> Self {
        Self::Tool {
            name: name.into(),
            args,
            status: ToolBlockStatus::Running,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prose_block_complete() {
        let block = StreamBlock::prose("Hello world");
        assert!(block.is_complete());
        assert_eq!(block.text(), "Hello world");
        assert_eq!(block.lang(), None);
    }

    #[test]
    fn test_prose_partial() {
        let mut block = StreamBlock::prose_partial("Hel");
        assert!(!block.is_complete());
        assert_eq!(block.text(), "Hel");

        // Complete it
        block.complete();
        assert!(block.is_complete());
    }

    #[test]
    fn test_prose_append() {
        let mut block = StreamBlock::prose_partial("Hello");
        block.append(" world");
        assert_eq!(block.text(), "Hello world");
        assert!(!block.is_complete()); // Still partial
    }

    #[test]
    fn test_code_block_with_lang() {
        let block = StreamBlock::code(Some("rust".into()), "fn main() {}");
        assert!(block.is_complete());
        assert_eq!(block.text(), "fn main() {}");
        assert_eq!(block.lang(), Some("rust"));
    }

    #[test]
    fn test_code_block_no_lang() {
        let block = StreamBlock::code(None, "plain code");
        assert!(block.is_complete());
        assert_eq!(block.text(), "plain code");
        assert_eq!(block.lang(), None);
    }

    #[test]
    fn test_code_partial() {
        let mut block = StreamBlock::code_partial(Some("python".into()), "def ");
        assert!(!block.is_complete());
        assert_eq!(block.lang(), Some("python"));

        block.append("main():");
        assert_eq!(block.text(), "def main():");

        block.complete();
        assert!(block.is_complete());
    }

    #[test]
    fn test_parse_event_text() {
        let event = ParseEvent::Text("Hello".into());
        assert!(matches!(event, ParseEvent::Text(_)));
    }

    #[test]
    fn test_parse_event_code_start_with_lang() {
        let event = ParseEvent::CodeBlockStart {
            lang: Some("rust".into()),
        };
        assert!(matches!(event, ParseEvent::CodeBlockStart { .. }));
        if let ParseEvent::CodeBlockStart { lang } = event {
            assert_eq!(lang, Some("rust".into()));
        }
    }

    #[test]
    fn test_parse_event_code_start_no_lang() {
        let event = ParseEvent::CodeBlockStart { lang: None };
        assert!(matches!(event, ParseEvent::CodeBlockStart { lang: None }));
    }

    #[test]
    fn test_parse_event_code_content() {
        let event = ParseEvent::CodeBlockContent("code here".into());
        assert!(matches!(event, ParseEvent::CodeBlockContent(_)));
    }

    #[test]
    fn test_parse_event_code_end() {
        let event = ParseEvent::CodeBlockEnd;
        assert!(matches!(event, ParseEvent::CodeBlockEnd));
    }

    #[test]
    fn test_parse_event_equality() {
        let e1 = ParseEvent::Text("hello".into());
        let e2 = ParseEvent::Text("hello".into());
        let e3 = ParseEvent::Text("world".into());

        assert_eq!(e1, e2);
        assert_ne!(e1, e3);
    }
}
