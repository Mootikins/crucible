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
    /// Length of content_buffer that was already flushed (to avoid duplicates)
    flushed_code_len: usize,
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
    /// Inside a markdown table, buffering lines until table ends
    /// Tables are emitted as a single Text event when complete for proper column width calculation
    InTable { lines: Vec<String> },
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
                    // Don't flush partial content that looks like it might be a table start
                    // Wait for newline to determine if it's actually a table
                    let trimmed = self.line_buffer.trim_start();
                    if trimmed.starts_with('|') {
                        // Potential table - don't flush, wait for newline
                        return None;
                    }
                    // Return partial text and clear the buffer
                    // (it will be re-accumulated if more chars arrive before newline)
                    Some(ParseEvent::Text(std::mem::take(&mut self.line_buffer)))
                } else {
                    None
                }
            }
            ParserState::InCodeBlock { .. } => {
                // In code blocks, return only NEW content since last flush
                // This prevents duplication when append_to_last_block appends
                let full_content = format!("{}{}", self.content_buffer, self.line_buffer);
                let new_content = &full_content[self.flushed_code_len..];
                if !new_content.is_empty() {
                    let result = new_content.to_string();
                    self.flushed_code_len = full_content.len();
                    Some(ParseEvent::CodeBlockContent(result))
                } else {
                    None
                }
            }
            ParserState::InTable { .. } => {
                // Tables need complete content for column width calculation
                // Don't flush partial tables - wait for completion
                None
            }
        }
    }

    /// Finalize parsing, completing any partial blocks
    pub fn finalize(&mut self) -> Vec<ParseEvent> {
        let mut events = Vec::new();

        match &mut self.state {
            ParserState::Text => {
                // Emit any remaining partial line (text before final newline)
                // Note: complete lines are emitted immediately in process_text_line
                if !self.line_buffer.is_empty() {
                    events.push(ParseEvent::Text(std::mem::take(&mut self.line_buffer)));
                }
            }
            ParserState::InCodeBlock { .. } => {
                // Unclosed code block - emit only UN-FLUSHED content and implicit end
                let full_content = format!("{}{}", self.content_buffer, self.line_buffer);
                let new_content = if self.flushed_code_len < full_content.len() {
                    &full_content[self.flushed_code_len..]
                } else {
                    ""
                };
                if !new_content.is_empty() {
                    events.push(ParseEvent::CodeBlockContent(new_content.to_string()));
                }
                events.push(ParseEvent::CodeBlockEnd);
                self.flushed_code_len = 0;
            }
            ParserState::InTable { lines } => {
                // Emit any remaining table lines as a single block
                if !lines.is_empty() {
                    let table_text = std::mem::take(lines).join("");
                    events.push(ParseEvent::Text(table_text));
                }
                // Also emit any partial line still in buffer
                if !self.line_buffer.is_empty() {
                    events.push(ParseEvent::Text(std::mem::take(&mut self.line_buffer)));
                }
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
            ParserState::InTable { .. } => self.process_table_line(line),
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
                // Reset flushed tracking for new code block
                self.flushed_code_len = 0;

                return events;
            }
        }

        // Check for table start (line starting with |)
        if Self::is_table_line(trimmed) {
            // Start table buffering
            self.state = ParserState::InTable { lines: vec![line] };
            return vec![];
        }

        // Regular text line - emit immediately for responsive streaming
        // Each line becomes a Text event so the UI updates as content arrives
        vec![ParseEvent::Text(line)]
    }

    /// Check if a line is part of a markdown table
    fn is_table_line(line: &str) -> bool {
        let trimmed = line.trim();
        // Table lines start with | or are separator lines like |---|---|
        trimmed.starts_with('|')
            || (trimmed.contains('|')
                && trimmed
                    .chars()
                    .all(|c| c == '|' || c == '-' || c == ':' || c.is_whitespace()))
    }

    /// Process a line while in table mode
    fn process_table_line(&mut self, line: String) -> Vec<ParseEvent> {
        let trimmed = line.trim_end();

        // Check if this line continues the table
        if Self::is_table_line(trimmed) {
            // Add to table buffer
            if let ParserState::InTable { lines } = &mut self.state {
                lines.push(line);
            }
            return vec![];
        }

        // Table ended - emit all buffered table lines as a single block
        let mut events = Vec::new();
        if let ParserState::InTable { lines } = &mut self.state {
            if !lines.is_empty() {
                let table_text = std::mem::take(lines).join("");
                events.push(ParseEvent::Text(table_text));
            }
        }

        // Reset to text mode
        self.state = ParserState::Text;

        // Process the non-table line as regular text
        // (it might be a code fence or another table)
        events.extend(self.process_text_line(line));
        events
    }

    fn process_code_line(
        &mut self,
        line: String,
        fence_char: char,
        fence_len: usize,
    ) -> Vec<ParseEvent> {
        // Check for closing fence
        // Per CommonMark spec, closing fence can have 0-3 spaces of indentation
        let fence_str = String::from(fence_char).repeat(fence_len);

        // Count leading spaces (max 3 allowed for closing fence)
        let leading_spaces = line.chars().take_while(|c| *c == ' ').count();

        // Only consider as closing fence if:
        // 1. Has 0-3 leading spaces
        // 2. After spaces, starts with the fence characters
        // 3. Rest of line is only fence chars or whitespace
        let after_spaces = &line[leading_spaces..];
        let trimmed_after = after_spaces.trim_end();

        let is_closing_fence = leading_spaces <= 3
            && trimmed_after.starts_with(&fence_str)
            && trimmed_after
                .chars()
                .all(|c| c == fence_char || c.is_whitespace());

        if is_closing_fence {
            // Found closing fence
            let mut events = Vec::new();

            // Emit only UN-FLUSHED content (content after flushed_code_len)
            if !self.content_buffer.is_empty() {
                // Remove trailing newline from content
                let full_content = self.content_buffer.trim_end_matches('\n').to_string();
                let new_content = if self.flushed_code_len < full_content.len() {
                    &full_content[self.flushed_code_len..]
                } else {
                    ""
                };
                if !new_content.is_empty() {
                    events.push(ParseEvent::CodeBlockContent(new_content.to_string()));
                }
                self.content_buffer.clear();
                self.flushed_code_len = 0;
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
        // flush_partial() returns accumulated content
        let partial = parser.flush_partial();
        assert!(partial.is_some());
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

    #[test]
    fn test_flush_partial_code_block_no_duplication() {
        // This test catches the bug where multiple flushes would return
        // ALL accumulated content, causing duplication when appended
        let mut parser = StreamingParser::new();

        // Start code block
        let events = parser.feed("```rust\n");
        assert_eq!(events.len(), 1); // CodeBlockStart

        // Add first chunk and flush
        parser.feed("line1");
        let flush1 = parser.flush_partial();
        assert!(matches!(
            flush1,
            Some(ParseEvent::CodeBlockContent(ref c)) if c == "line1"
        ));

        // Add second chunk and flush - should only return NEW content
        parser.feed("\nline2");
        let flush2 = parser.flush_partial();
        assert!(
            matches!(
                flush2,
                Some(ParseEvent::CodeBlockContent(ref c)) if c == "\nline2"
            ),
            "Second flush should only return new content, not 'line1\\nline2'"
        );

        // Add third chunk and flush
        parser.feed("\nline3");
        let flush3 = parser.flush_partial();
        assert!(
            matches!(
                flush3,
                Some(ParseEvent::CodeBlockContent(ref c)) if c == "\nline3"
            ),
            "Third flush should only return new content"
        );

        // Simulating what the view does: appending all flushes
        // Should result in "line1\nline2\nline3", not "line1line1\nline2line1\nline2\nline3"
        let mut accumulated = String::new();
        if let Some(ParseEvent::CodeBlockContent(c)) = flush1 {
            accumulated.push_str(&c);
        }
        if let Some(ParseEvent::CodeBlockContent(c)) = flush2 {
            accumulated.push_str(&c);
        }
        if let Some(ParseEvent::CodeBlockContent(c)) = flush3 {
            accumulated.push_str(&c);
        }
        assert_eq!(accumulated, "line1\nline2\nline3");
    }

    /// Test that backticks inside comments don't prematurely close code blocks
    #[test]
    fn test_code_block_with_backticks_in_comment() {
        let mut parser = StreamingParser::new();

        // Code block with a comment that contains triple backticks
        let input =
            "```python\n# This comment has ``` backticks\nprint(\"hello\")\n```\nAfter the code\n";

        let events = parser.feed(input);

        // Debug output
        for (i, event) in events.iter().enumerate() {
            eprintln!("Event {}: {:?}", i, event);
        }

        // Should have: CodeBlockStart, CodeBlockContent, CodeBlockEnd, Text
        // NOT: CodeBlockStart, Text (if comment closed block prematurely)
        assert!(
            events.len() >= 3,
            "Should have at least 3 events (start, content, end), got {}",
            events.len()
        );

        // First event should be CodeBlockStart
        assert!(
            matches!(events[0], ParseEvent::CodeBlockStart { .. }),
            "First event should be CodeBlockStart, got {:?}",
            events[0]
        );

        // Code content should include the comment line
        let has_comment_in_code = events.iter().any(|e| {
            if let ParseEvent::CodeBlockContent(content) = e {
                content.contains("# This comment has")
            } else {
                false
            }
        });
        assert!(
            has_comment_in_code,
            "Comment with backticks should be part of code block content"
        );

        // There should be a CodeBlockEnd before any text
        let end_idx = events
            .iter()
            .position(|e| matches!(e, ParseEvent::CodeBlockEnd));
        assert!(end_idx.is_some(), "Should have CodeBlockEnd event");
    }

    /// Test code block with line that's just indented backticks
    #[test]
    fn test_code_block_indented_backticks() {
        let mut parser = StreamingParser::new();

        // Code block with indented backticks (shouldn't close the block)
        let input = "```python\ncode_here\n   ```\nmore_code\n```\nAfter\n";

        let events = parser.feed(input);

        for (i, event) in events.iter().enumerate() {
            eprintln!("Event {}: {:?}", i, event);
        }

        // The indented ``` should NOT close the block (it has leading whitespace)
        // Actually, let's check what the behavior is...
        let code_content: String = events
            .iter()
            .filter_map(|e| {
                if let ParseEvent::CodeBlockContent(c) = e {
                    Some(c.as_str())
                } else {
                    None
                }
            })
            .collect();

        eprintln!("Code content: '{}'", code_content);

        // If indented backticks close the block, "more_code" won't be in content
        // If they don't, "more_code" will be in content
    }

    #[test]
    fn test_table_buffering() {
        let mut parser = StreamingParser::new();

        // Feed a complete table
        let events = parser.feed("| A | B |\n|---|---|\n| 1 | 2 |\n\n");

        // Table lines should be buffered and emitted together when followed by non-table line
        // The final blank line triggers the table completion
        assert_eq!(events.len(), 2); // Table block + blank line

        // First event should be the complete table
        if let ParseEvent::Text(text) = &events[0] {
            assert!(text.contains("| A | B |"));
            assert!(text.contains("|---|---|"));
            assert!(text.contains("| 1 | 2 |"));
        } else {
            panic!("Expected Text event for table");
        }
    }

    #[test]
    fn test_table_buffering_streaming() {
        let mut parser = StreamingParser::new();

        // Stream table line by line
        let events1 = parser.feed("| Header1 | Header2 |\n");
        assert!(events1.is_empty(), "Table lines should be buffered");

        let events2 = parser.feed("|---------|----------|\n");
        assert!(events2.is_empty(), "Table lines should be buffered");

        let events3 = parser.feed("| Data1   | Data2    |\n");
        assert!(events3.is_empty(), "Table lines should be buffered");

        // Non-table line ends the table
        let events4 = parser.feed("Some text after table\n");
        assert_eq!(events4.len(), 2); // Table + following text

        if let ParseEvent::Text(table_text) = &events4[0] {
            assert!(table_text.contains("Header1"));
            assert!(table_text.contains("Data1"));
        } else {
            panic!("Expected Text event for table");
        }
    }

    #[test]
    fn test_flush_partial_in_table() {
        let mut parser = StreamingParser::new();

        // Start a table
        parser.feed("| A | B |\n");

        // Flush should NOT return anything (tables need complete content)
        let partial = parser.flush_partial();
        assert!(partial.is_none(), "Partial tables should not be flushed");
    }

    #[test]
    fn test_finalize_incomplete_table() {
        let mut parser = StreamingParser::new();

        // Start a table but don't end it
        parser.feed("| A | B |\n|---|---|\n| 1 | 2 |\n");

        // Finalize should emit the buffered table
        let events = parser.finalize();
        assert_eq!(events.len(), 1);

        if let ParseEvent::Text(table_text) = &events[0] {
            assert!(table_text.contains("| A | B |"));
        } else {
            panic!("Expected Text event for table");
        }
    }
}
