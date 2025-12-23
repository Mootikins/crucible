//! Streaming buffer for accumulating TextDelta events with flush detection
//!
//! Collects streaming tokens and detects natural break points for
//! incremental flushing to terminal scrollback.

use std::time::{Duration, Instant};

/// Buffer for accumulating streaming response tokens with flush detection.
///
/// Detects natural break points (paragraph breaks, code block ends) and
/// capacity limits to trigger incremental flushes to terminal scrollback.
#[derive(Debug, Clone, Copy, PartialEq)]
enum FenceType {
    Backtick,
    Tilde,
}

#[derive(Debug)]
pub struct StreamingBuffer {
    /// Content waiting to be flushed
    buffer: String,
    /// Content already flushed (for reference)
    flushed: String,
    /// Maximum lines before capacity flush (0 = unlimited)
    max_lines: usize,
    /// When streaming started
    started_at: Instant,
    /// Track if we're inside a code block (computed on each check)
    in_code_block: bool,
    /// Type of fence that opened the current code block
    fence_type: Option<FenceType>,
}

impl StreamingBuffer {
    /// Create a new streaming buffer with default settings
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            flushed: String::new(),
            max_lines: 0,
            started_at: Instant::now(),
            in_code_block: false,
            fence_type: None,
        }
    }

    /// Create a new streaming buffer with a max line capacity
    pub fn with_max_lines(max_lines: usize) -> Self {
        Self {
            buffer: String::new(),
            flushed: String::new(),
            max_lines,
            started_at: Instant::now(),
            in_code_block: false,
            fence_type: None,
        }
    }

    /// Check if the buffer is empty
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Get the current buffered content (not yet flushed)
    pub fn content(&self) -> &str {
        &self.buffer
    }

    /// Get all content (flushed + buffered)
    pub fn all_content(&self) -> String {
        format!("{}{}", self.flushed, self.buffer)
    }

    /// Get elapsed time since streaming started
    pub fn elapsed(&self) -> Duration {
        self.started_at.elapsed()
    }

    /// Append content to the buffer, returning content to flush if a break point is detected.
    ///
    /// Flush is triggered by:
    /// - Paragraph break (`\n\n`)
    /// - Code block end (``` followed by newline, when in code block)
    /// - Capacity limit (80% of max_lines reached)
    ///
    /// Returns `Some(content)` if content should be flushed, `None` otherwise.
    pub fn append(&mut self, delta: &str) -> Option<String> {
        self.buffer.push_str(delta);
        self.check_flush()
    }

    /// Check if buffer should be flushed and return flush content if so
    fn check_flush(&mut self) -> Option<String> {
        // First, check if there's a complete code block we can flush
        // This handles the case where a code block was opened and closed
        if let Some(pos) = self.find_complete_code_block() {
            let result = self.flush_up_to(pos);
            self.update_code_block_state(); // Update state after flush
            return Some(result);
        }

        // Update code block state for current buffer
        self.update_code_block_state();

        // If inside a code block, don't flush anything else
        if self.in_code_block {
            return None;
        }

        // Outside code block: check for paragraph break
        if let Some(pos) = self.find_paragraph_break() {
            return Some(self.flush_up_to(pos + 2)); // Include the \n\n
        }

        // Check for capacity flush
        if self.max_lines > 0 {
            let line_count = self.buffer.lines().count();
            let threshold = (self.max_lines * 80) / 100;
            if line_count >= threshold && threshold > 0 {
                // Flush up to the last complete line
                if let Some(pos) = self.buffer.rfind('\n') {
                    return Some(self.flush_up_to(pos + 1));
                }
            }
        }

        None
    }

    /// Find paragraph break position in buffer
    fn find_paragraph_break(&self) -> Option<usize> {
        self.buffer.find("\n\n")
    }

    /// Find a complete code block (opening + closing fence + newline after closing).
    /// Returns the position after the closing fence's newline.
    fn find_complete_code_block(&self) -> Option<usize> {
        let mut in_block = false;
        let mut current_fence: Option<FenceType> = None;
        let mut pos = 0;

        while pos < self.buffer.len() {
            let at_line_start = pos == 0 || self.buffer.as_bytes().get(pos - 1) == Some(&b'\n');

            if at_line_start {
                let remaining = &self.buffer[pos..];
                let fence = if remaining.starts_with("```") {
                    Some(FenceType::Backtick)
                } else if remaining.starts_with("~~~") {
                    Some(FenceType::Tilde)
                } else {
                    None
                };

                if let Some(found_fence) = fence {
                    let fence_end = pos + 3;

                    if !in_block {
                        // Opening a code block
                        in_block = true;
                        current_fence = Some(found_fence);
                    } else if current_fence == Some(found_fence) {
                        // Closing with matching fence type
                        // Check for newline after closing fence
                        if let Some(newline) = self.buffer[fence_end..].find('\n') {
                            return Some(fence_end + newline + 1);
                        }
                        // No newline yet - can't flush
                        return None;
                    }
                    // Mismatched fence type - ignore

                    pos = fence_end;
                    if let Some(newline) = self.buffer[pos..].find('\n') {
                        pos += newline + 1;
                    } else {
                        break;
                    }
                    continue;
                }
            }

            if let Some(newline) = self.buffer[pos..].find('\n') {
                pos += newline + 1;
            } else {
                break;
            }
        }

        None
    }

    /// Update code block state by scanning for fence markers.
    /// This should be called before checking for flushes.
    fn update_code_block_state(&mut self) {
        // Count fence markers to determine if we're in a code block
        // A fence is ``` or ~~~ at the start of a line (or start of buffer)
        // Fences must match: ``` closes ```, ~~~ closes ~~~
        let mut in_block = false;
        let mut current_fence: Option<FenceType> = None;
        let mut pos = 0;

        while pos < self.buffer.len() {
            // Check if we're at start of line (or start of buffer)
            let at_line_start = pos == 0 || self.buffer.as_bytes().get(pos - 1) == Some(&b'\n');

            if at_line_start {
                let remaining = &self.buffer[pos..];
                let fence = if remaining.starts_with("```") {
                    Some(FenceType::Backtick)
                } else if remaining.starts_with("~~~") {
                    Some(FenceType::Tilde)
                } else {
                    None
                };

                if let Some(found_fence) = fence {
                    if !in_block {
                        // Opening a code block
                        in_block = true;
                        current_fence = Some(found_fence);
                    } else if current_fence == Some(found_fence) {
                        // Closing with matching fence type
                        in_block = false;
                        current_fence = None;
                    }
                    // If fence type doesn't match, ignore it (stays in block)

                    pos += 3;
                    // Skip to end of line
                    if let Some(newline) = self.buffer[pos..].find('\n') {
                        pos += newline + 1;
                    } else {
                        break;
                    }
                    continue;
                }
            }

            // Move to next line
            if let Some(newline) = self.buffer[pos..].find('\n') {
                pos += newline + 1;
            } else {
                break;
            }
        }

        self.in_code_block = in_block;
        self.fence_type = current_fence;
    }

    /// Flush content up to the given byte position
    fn flush_up_to(&mut self, pos: usize) -> String {
        let to_flush = self.buffer[..pos].to_string();
        self.buffer = self.buffer[pos..].to_string();
        self.flushed.push_str(&to_flush);
        to_flush
    }

    /// Finalize the buffer, returning any remaining content
    ///
    /// This should be called when the response is complete.
    pub fn finalize(&mut self) -> String {
        let remaining = std::mem::take(&mut self.buffer);
        self.flushed.push_str(&remaining);
        remaining
    }
}

impl Default for StreamingBuffer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 1.1.1: Test StreamingBuffer creation
    #[test]
    fn test_streaming_buffer_creation() {
        let buf = StreamingBuffer::new();
        assert!(buf.is_empty());
        assert_eq!(buf.content(), "");
        assert!(buf.flushed.is_empty());
    }

    #[test]
    fn test_streaming_buffer_with_max_lines() {
        let buf = StreamingBuffer::with_max_lines(10);
        assert!(buf.is_empty());
        assert_eq!(buf.max_lines, 10);
    }

    // 1.2.1: Test paragraph break detection
    #[test]
    fn test_paragraph_break_detection() {
        let mut buf = StreamingBuffer::new();

        // Append content without paragraph break - no flush
        let result = buf.append("Hello");
        assert!(result.is_none());
        assert_eq!(buf.content(), "Hello");

        // Append paragraph break - should flush
        let result = buf.append("\n\nWorld");
        assert!(result.is_some());
        assert_eq!(result.unwrap(), "Hello\n\n");
        assert_eq!(buf.content(), "World");
    }

    #[test]
    fn test_paragraph_break_at_once() {
        let mut buf = StreamingBuffer::new();
        let result = buf.append("Hello\n\nWorld");
        assert!(result.is_some());
        assert_eq!(result.unwrap(), "Hello\n\n");
        assert_eq!(buf.content(), "World");
    }

    // 1.2.2: Test code block end detection
    #[test]
    fn test_code_block_end_detection() {
        let mut buf = StreamingBuffer::new();

        // Opening fence - no flush
        let result = buf.append("```rust\ncode here\n");
        assert!(result.is_none());
        assert!(buf.in_code_block);

        // Closing fence with newline - should flush
        let result = buf.append("```\nMore text");
        assert!(result.is_some());
        let flushed = result.unwrap();
        assert!(flushed.contains("```rust"));
        assert!(flushed.contains("code here"));
        assert!(flushed.ends_with("```\n"));
        assert_eq!(buf.content(), "More text");
        assert!(!buf.in_code_block);
    }

    #[test]
    fn test_code_block_no_flush_without_newline() {
        let mut buf = StreamingBuffer::new();
        buf.append("```rust\ncode\n```");
        // No newline after closing fence - shouldn't flush yet
        assert!(!buf.in_code_block); // Block is closed
        assert!(buf.content().contains("```rust")); // But not flushed
    }

    // 1.2.3: Test capacity flush
    #[test]
    fn test_capacity_flush() {
        let mut buf = StreamingBuffer::with_max_lines(10);

        // Add lines one at a time
        for i in 0..7 {
            let result = buf.append(&format!("Line {}\n", i));
            assert!(result.is_none(), "Should not flush before 80% capacity");
        }

        // At 8 lines (80% of 10), should flush
        let result = buf.append("Line 7\n");
        assert!(result.is_some(), "Should flush at 80% capacity");
        let flushed = result.unwrap();
        assert!(flushed.contains("Line 0"));
    }

    #[test]
    fn test_capacity_flush_15_lines() {
        let mut buf = StreamingBuffer::with_max_lines(10);

        // Build up content with 15 lines
        let mut content = String::new();
        for i in 0..15 {
            content.push_str(&format!("Line {}\n", i));
        }

        let result = buf.append(&content);
        // Should have flushed since we exceeded 80%
        assert!(result.is_some());
    }

    // 1.3.1: Test finalize
    #[test]
    fn test_finalize_returns_remaining() {
        let mut buf = StreamingBuffer::new();
        buf.append("Hello ");
        buf.append("world");

        let remaining = buf.finalize();
        assert_eq!(remaining, "Hello world");
        assert!(buf.is_empty());
    }

    #[test]
    fn test_finalize_after_flush() {
        let mut buf = StreamingBuffer::new();
        buf.append("First\n\n");
        // First part flushed
        let _result = buf.append("Second");
        // No flush for "Second" alone

        let remaining = buf.finalize();
        assert_eq!(remaining, "Second");
        assert_eq!(buf.all_content(), "First\n\nSecond");
    }

    #[test]
    fn test_finalize_clears_buffer() {
        let mut buf = StreamingBuffer::new();
        buf.append("content");
        buf.finalize();
        assert!(buf.is_empty());
        assert_eq!(buf.content(), "");
    }

    // Additional edge case tests
    #[test]
    fn test_multiple_paragraph_breaks() {
        let mut buf = StreamingBuffer::new();

        // First paragraph break triggers flush of "A\n\n"
        let result = buf.append("A\n\nB\n\nC");
        assert_eq!(result, Some("A\n\n".to_string()));
        assert_eq!(buf.content(), "B\n\nC");

        // Second append checks for flush again - finds "B\n\n"
        let result = buf.append("");
        assert_eq!(result, Some("B\n\n".to_string()));
        assert_eq!(buf.content(), "C");
    }

    #[test]
    fn test_elapsed_time() {
        let buf = StreamingBuffer::new();
        std::thread::sleep(Duration::from_millis(10));
        assert!(buf.elapsed() >= Duration::from_millis(10));
    }

    #[test]
    fn test_all_content_tracks_flushed() {
        let mut buf = StreamingBuffer::new();
        buf.append("First\n\nSecond");
        assert_eq!(buf.all_content(), "First\n\nSecond");

        // After the flush from append
        assert_eq!(buf.flushed, "First\n\n");
        assert_eq!(buf.content(), "Second");
    }

    // Bug fix tests
    #[test]
    fn test_paragraph_break_inside_code_block_no_flush() {
        let mut buf = StreamingBuffer::new();

        // Code block with blank line inside - should NOT flush on the blank line
        let result = buf.append("```rust\nfn foo() {\n\n}\n```\n");
        // Should flush the entire code block, not prematurely on \n\n
        assert!(result.is_some());
        let flushed = result.unwrap();
        assert!(flushed.contains("fn foo()"));
        assert!(flushed.contains("}"));
        assert!(flushed.ends_with("```\n"));
    }

    #[test]
    fn test_paragraph_break_inside_code_block_streaming() {
        let mut buf = StreamingBuffer::new();

        // Stream in parts - opening fence
        let result = buf.append("```\n");
        assert!(result.is_none());
        assert!(buf.in_code_block);

        // Stream in blank line - should NOT flush
        let result = buf.append("line1\n\nline2\n");
        assert!(
            result.is_none(),
            "Should not flush paragraph break inside code block"
        );
        assert!(buf.in_code_block);

        // Stream in closing fence
        let result = buf.append("```\nafter");
        assert!(result.is_some());
        let flushed = result.unwrap();
        assert!(flushed.contains("line1\n\nline2"));
        assert_eq!(buf.content(), "after");
    }

    #[test]
    fn test_tilde_code_block() {
        let mut buf = StreamingBuffer::new();

        let result = buf.append("~~~python\nprint('hello')\n~~~\ntext");
        assert!(result.is_some());
        let flushed = result.unwrap();
        assert!(flushed.contains("~~~python"));
        assert!(flushed.contains("print('hello')"));
        assert!(flushed.ends_with("~~~\n"));
        assert_eq!(buf.content(), "text");
    }

    #[test]
    fn test_mixed_fences_not_matched() {
        let mut buf = StreamingBuffer::new();

        // Opening with backticks, "closing" with tildes - should still be in code block
        buf.append("```rust\ncode\n~~~\n");
        assert!(buf.in_code_block, "Mismatched fence should not close block");
    }
}
