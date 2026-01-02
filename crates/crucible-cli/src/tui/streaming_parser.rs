//! Incremental markdown parser for streaming LLM responses

use super::content_block::{ParseEvent, StreamBlock};

/// Parser state machine for incremental markdown parsing
#[derive(Debug, Default)]
pub struct StreamingParser {
    state: ParserState,
    /// Current line being accumulated
    line_buffer: String,
    /// Content accumulated so far (for code blocks)
    content_buffer: String,
    /// Accumulated content blocks
    blocks: Vec<StreamBlock>,
}

#[derive(Debug, Default, Clone, PartialEq)]
enum ParserState {
    #[default]
    Text,
    /// Inside a code fence, reading content
    InCodeBlock {
        lang: Option<String>,
        fence_char: char,
        fence_len: usize,
    },
}

impl StreamingParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed a token delta, returning parse events
    pub fn feed(&mut self, delta: &str) -> Vec<ParseEvent> {
        let mut events = Vec::new();

        for ch in delta.chars() {
            let mut char_events = self.process_char(ch);
            events.append(&mut char_events);
        }

        events
    }

    /// Get current blocks (for rendering)
    pub fn blocks(&self) -> &[StreamBlock] {
        &self.blocks
    }

    /// Get partial content being accumulated
    pub fn partial(&self) -> String {
        // Return whatever's in our buffers
        format!("{}{}", self.line_buffer, self.content_buffer)
    }

    /// Check if currently in a code block
    pub fn in_code_block(&self) -> bool {
        matches!(self.state, ParserState::InCodeBlock { .. })
    }

    /// Flush partial text for progressive display.
    ///
    /// In text mode, returns the current line buffer as a Text event without
    /// consuming it (the buffer is marked as "flushed" to avoid duplicates).
    /// In code block mode, returns the accumulated code content.
    ///
    /// This enables progressive display of streaming text even before newlines.
    pub fn flush_partial(&mut self) -> Option<ParseEvent> {
        match &self.state {
            ParserState::Text => {
                if !self.line_buffer.is_empty() {
                    // Return partial text and clear the buffer
                    // (it will be re-accumulated if more chars arrive before newline)
                    Some(ParseEvent::Text(std::mem::take(&mut self.line_buffer)))
                } else {
                    None
                }
            }
            ParserState::InCodeBlock { .. } => {
                // In code blocks, we can show partial code content
                let content = format!("{}{}", self.content_buffer, self.line_buffer);
                if !content.is_empty() {
                    // Don't clear buffers - this is just for display
                    // The content_buffer is for proper parsing
                    Some(ParseEvent::CodeBlockContent(content.clone()))
                } else {
                    None
                }
            }
        }
    }

    /// Finalize parsing, completing any partial blocks
    pub fn finalize(&mut self) -> Vec<ParseEvent> {
        let mut events = Vec::new();

        match &self.state {
            ParserState::Text => {
                // Emit any remaining partial line (text before final newline)
                // Note: complete lines are emitted immediately in process_text_line
                if !self.line_buffer.is_empty() {
                    events.push(ParseEvent::Text(std::mem::take(&mut self.line_buffer)));
                }
            }
            ParserState::InCodeBlock { .. } => {
                // Unclosed code block - emit content and implicit end
                let content = format!("{}{}", self.content_buffer, self.line_buffer);
                if !content.is_empty() {
                    events.push(ParseEvent::CodeBlockContent(content));
                }
                events.push(ParseEvent::CodeBlockEnd);
            }
        }

        self.line_buffer.clear();
        self.content_buffer.clear();
        self.state = ParserState::Text;
        events
    }

    fn process_char(&mut self, ch: char) -> Vec<ParseEvent> {
        self.line_buffer.push(ch);

        if ch == '\n' {
            // End of line - check for special markers
            return self.process_line_end();
        }

        vec![]
    }

    fn process_line_end(&mut self) -> Vec<ParseEvent> {
        let line = std::mem::take(&mut self.line_buffer);

        match &self.state {
            ParserState::Text => self.process_text_line(line),
            ParserState::InCodeBlock {
                fence_char,
                fence_len,
                ..
            } => self.process_code_line(line, *fence_char, *fence_len),
        }
    }

    fn process_text_line(&mut self, line: String) -> Vec<ParseEvent> {
        let trimmed = line.trim_end();

        // Check for code fence start
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            let fence_char = if trimmed.starts_with("```") { '`' } else { '~' };
            let fence_len = trimmed.chars().take_while(|&c| c == fence_char).count();

            if fence_len >= 3 {
                // Extract language
                let lang_part = &trimmed[fence_len..];
                let lang = lang_part.trim();
                let lang = if lang.is_empty() {
                    None
                } else {
                    Some(lang.to_string())
                };

                // Emit any accumulated text before the fence
                let mut events = Vec::new();
                if !self.content_buffer.is_empty() {
                    events.push(ParseEvent::Text(std::mem::take(&mut self.content_buffer)));
                }

                // Start code block
                events.push(ParseEvent::CodeBlockStart { lang: lang.clone() });
                self.state = ParserState::InCodeBlock {
                    lang,
                    fence_char,
                    fence_len,
                };

                return events;
            }
        }

        // Regular text line - emit immediately for responsive streaming
        // Each line becomes a Text event so the UI updates as content arrives
        vec![ParseEvent::Text(line)]
    }

    fn process_code_line(
        &mut self,
        line: String,
        fence_char: char,
        fence_len: usize,
    ) -> Vec<ParseEvent> {
        let trimmed = line.trim();

        // Check for closing fence
        let fence_str = &String::from(fence_char).repeat(fence_len);
        if trimmed.starts_with(fence_str)
            && trimmed
                .chars()
                .all(|c| c == fence_char || c.is_whitespace())
        {
            // Found closing fence
            let mut events = Vec::new();

            // Emit content if any
            if !self.content_buffer.is_empty() {
                // Remove trailing newline from content
                let content = self.content_buffer.trim_end_matches('\n').to_string();
                if !content.is_empty() {
                    events.push(ParseEvent::CodeBlockContent(content));
                }
                self.content_buffer.clear();
            }

            // Emit end marker
            events.push(ParseEvent::CodeBlockEnd);
            self.state = ParserState::Text;

            return events;
        }

        // Regular code line - accumulate
        self.content_buffer.push_str(&line);
        vec![]
    }

    /// Apply parse events to build content blocks (for internal use or rendering)
    pub fn apply_events(&mut self, events: &[ParseEvent]) {
        for event in events {
            match event {
                ParseEvent::Text(text) => {
                    self.blocks.push(StreamBlock::prose(text.clone()));
                }
                ParseEvent::CodeBlockStart { lang } => {
                    self.blocks
                        .push(StreamBlock::code_partial(lang.clone(), ""));
                }
                ParseEvent::CodeBlockContent(content) => {
                    if let Some(block) = self.blocks.last_mut() {
                        block.append(content);
                    }
                }
                ParseEvent::CodeBlockEnd => {
                    if let Some(block) = self.blocks.last_mut() {
                        block.complete();
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plain_text() {
        let mut parser = StreamingParser::new();
        let events = parser.feed("Hello world");
        assert!(events.is_empty()); // No events until finalize

        let events = parser.finalize();
        assert_eq!(events, vec![ParseEvent::Text("Hello world".into())]);
    }

    #[test]
    fn test_plain_text_multiline() {
        let mut parser = StreamingParser::new();
        // Lines are emitted immediately as they complete (on newline)
        let events = parser.feed("Line 1\nLine 2\nLine 3");

        // First two lines are complete (end with \n), third is partial
        assert_eq!(
            events,
            vec![
                ParseEvent::Text("Line 1\n".into()),
                ParseEvent::Text("Line 2\n".into()),
            ]
        );

        // Finalize emits the remaining partial line
        let final_events = parser.finalize();
        assert_eq!(final_events, vec![ParseEvent::Text("Line 3".into())]);
    }

    #[test]
    fn test_code_block_complete() {
        let mut parser = StreamingParser::new();

        // Feed complete code block
        let events = parser.feed("```rust\nfn main() {}\n```\n");

        // Should get: CodeBlockStart, CodeBlockContent, CodeBlockEnd
        assert_eq!(events.len(), 3);
        assert!(matches!(
            events[0],
            ParseEvent::CodeBlockStart {
                lang: Some(ref l)
            } if l == "rust"
        ));
        assert!(matches!(
            events[1],
            ParseEvent::CodeBlockContent(ref c) if c == "fn main() {}"
        ));
        assert!(matches!(events[2], ParseEvent::CodeBlockEnd));
    }

    #[test]
    fn test_code_block_no_language() {
        let mut parser = StreamingParser::new();

        let events = parser.feed("```\ncode\n```\n");

        assert_eq!(events.len(), 3);
        assert!(matches!(
            events[0],
            ParseEvent::CodeBlockStart { lang: None }
        ));
        assert!(matches!(
            events[1],
            ParseEvent::CodeBlockContent(ref c) if c == "code"
        ));
        assert!(matches!(events[2], ParseEvent::CodeBlockEnd));
    }

    #[test]
    fn test_streaming_tokens() {
        let mut parser = StreamingParser::new();

        // Stream tokens one at a time
        for ch in "Hello".chars() {
            let events = parser.feed(&ch.to_string());
            assert!(events.is_empty()); // No events until finalize for plain text
        }

        let events = parser.finalize();
        assert_eq!(events, vec![ParseEvent::Text("Hello".into())]);
    }

    #[test]
    fn test_streaming_code_block() {
        let mut parser = StreamingParser::new();

        // Stream code block token by token
        let input = "```rust\nfn main()\n```\n";
        let mut all_events = Vec::new();

        for ch in input.chars() {
            let events = parser.feed(&ch.to_string());
            all_events.extend(events);
        }

        // Should have all three events
        assert_eq!(all_events.len(), 3);
        assert!(matches!(
            all_events[0],
            ParseEvent::CodeBlockStart {
                lang: Some(ref l)
            } if l == "rust"
        ));
    }

    #[test]
    fn test_text_then_code() {
        let mut parser = StreamingParser::new();

        let events = parser.feed("Here's code:\n```rust\nfn main() {}\n```\n");

        // Should get: Text, CodeBlockStart, CodeBlockContent, CodeBlockEnd
        assert_eq!(events.len(), 4);
        assert!(matches!(
            events[0],
            ParseEvent::Text(ref t) if t == "Here's code:\n"
        ));
        assert!(matches!(
            events[1],
            ParseEvent::CodeBlockStart {
                lang: Some(ref l)
            } if l == "rust"
        ));
    }

    #[test]
    fn test_code_then_text() {
        let mut parser = StreamingParser::new();

        let events = parser.feed("```\ncode\n```\nMore text");

        // Code block completes, then text accumulates
        assert_eq!(events.len(), 3); // Start, Content, End

        let events = parser.finalize();
        assert_eq!(events, vec![ParseEvent::Text("More text".into())]);
    }

    #[test]
    fn test_unclosed_code_block() {
        let mut parser = StreamingParser::new();

        parser.feed("```rust\nfn main() {");

        let events = parser.finalize();
        // Should emit content and implicit end
        assert_eq!(events.len(), 2);
        assert!(matches!(
            events[0],
            ParseEvent::CodeBlockContent(ref c) if c == "fn main() {"
        ));
        assert!(matches!(events[1], ParseEvent::CodeBlockEnd));
    }

    #[test]
    fn test_multiple_code_blocks() {
        let mut parser = StreamingParser::new();

        let events = parser.feed("```rust\ncode1\n```\nText\n```python\ncode2\n```\n");

        // First block: 3 events
        // Text event
        // Second block: 3 events
        // Total: 7 events
        assert_eq!(events.len(), 7);
    }

    #[test]
    fn test_code_fence_with_tildes() {
        let mut parser = StreamingParser::new();

        let events = parser.feed("~~~rust\ncode\n~~~\n");

        assert_eq!(events.len(), 3);
        assert!(matches!(
            events[0],
            ParseEvent::CodeBlockStart {
                lang: Some(ref l)
            } if l == "rust"
        ));
    }

    #[test]
    fn test_partial_state() {
        let mut parser = StreamingParser::new();

        parser.feed("Hello");
        // Partial() includes accumulated content
        assert!(!parser.partial().is_empty());
        assert!(!parser.in_code_block());
    }

    #[test]
    fn test_in_code_block_state() {
        let mut parser = StreamingParser::new();

        parser.feed("```rust\n");
        assert!(parser.in_code_block());

        parser.feed("code\n```\n");
        assert!(!parser.in_code_block()); // Exited after closing fence
    }

    #[test]
    fn test_empty_code_block() {
        let mut parser = StreamingParser::new();

        let events = parser.feed("```\n```\n");

        assert_eq!(events.len(), 2); // Start and End, no content
        assert!(matches!(
            events[0],
            ParseEvent::CodeBlockStart { lang: None }
        ));
        assert!(matches!(events[1], ParseEvent::CodeBlockEnd));
    }

    #[test]
    fn test_code_block_multiline_content() {
        let mut parser = StreamingParser::new();

        let events = parser.feed("```rust\nfn main() {\n    println!(\"hello\");\n}\n```\n");

        assert_eq!(events.len(), 3);
        if let ParseEvent::CodeBlockContent(content) = &events[1] {
            assert_eq!(content, "fn main() {\n    println!(\"hello\");\n}");
        } else {
            panic!("Expected CodeBlockContent");
        }
    }

    #[test]
    fn test_language_with_extra_info() {
        let mut parser = StreamingParser::new();

        let events = parser.feed("```rust,ignore\ncode\n```\n");

        // Language line includes everything after fence
        assert!(matches!(
            events[0],
            ParseEvent::CodeBlockStart {
                lang: Some(ref l)
            } if l == "rust,ignore"
        ));
    }

    #[test]
    fn test_flush_partial_text() {
        let mut parser = StreamingParser::new();

        // Feed text without newline
        let events = parser.feed("Hello world");
        assert!(events.is_empty()); // No events yet (no newline)

        // Flush should return partial text
        let partial = parser.flush_partial();
        assert!(matches!(
            partial,
            Some(ParseEvent::Text(ref t)) if t == "Hello world"
        ));

        // After flush, buffer should be empty
        let partial2 = parser.flush_partial();
        assert!(partial2.is_none());
    }

    #[test]
    fn test_flush_partial_progressive() {
        let mut parser = StreamingParser::new();

        // Stream tokens progressively
        parser.feed("The ");
        let p1 = parser.flush_partial();
        assert!(matches!(p1, Some(ParseEvent::Text(ref t)) if t == "The "));

        parser.feed("answer ");
        let p2 = parser.flush_partial();
        assert!(matches!(p2, Some(ParseEvent::Text(ref t)) if t == "answer "));

        parser.feed("is 42");
        let p3 = parser.flush_partial();
        assert!(matches!(p3, Some(ParseEvent::Text(ref t)) if t == "is 42"));
    }

    #[test]
    fn test_flush_partial_with_newlines() {
        let mut parser = StreamingParser::new();

        // Feed line with newline
        let events = parser.feed("Line 1\n");
        assert_eq!(events.len(), 1); // Newline triggers immediate event

        // Buffer should be empty after newline
        let partial = parser.flush_partial();
        assert!(partial.is_none());

        // Feed partial line
        parser.feed("Line 2");
        let partial = parser.flush_partial();
        assert!(matches!(partial, Some(ParseEvent::Text(ref t)) if t == "Line 2"));
    }

    #[test]
    fn test_flush_partial_in_code_block() {
        let mut parser = StreamingParser::new();

        // Start code block
        let events = parser.feed("```rust\n");
        assert_eq!(events.len(), 1); // CodeBlockStart

        // Add partial code
        parser.feed("fn main() {");

        // Flush should return code content
        let partial = parser.flush_partial();
        assert!(matches!(
            partial,
            Some(ParseEvent::CodeBlockContent(ref c)) if c == "fn main() {"
        ));
    }
}
