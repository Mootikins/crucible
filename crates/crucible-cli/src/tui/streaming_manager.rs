//! Streaming subsystem manager
//!
//! Manages streaming content from LLM providers, including:
//! - Background streaming task and channel receiver
//! - Receiving streaming events from agent
//! - Parsing markdown chunks into structured blocks
//! - Managing pending chunks awaiting processing
//!
//! This consolidates all streaming-related state that was previously split
//! between RatatuiRunner fields and this manager.

use crate::tui::streaming::StreamingBuffer;
use crate::tui::streaming_channel::StreamingReceiver;
use crate::tui::streaming_parser::StreamingParser;

/// Manages streaming content from LLM
///
/// Owns all streaming-related state:
/// - `task`: Background tokio task receiving from LLM stream
/// - `rx`: Channel receiver for streaming events
/// - `buffer`: Accumulated streaming content
/// - `parser`: Markdown parser for structured block extraction
pub struct StreamingManager {
    /// Background streaming task handle
    task: Option<tokio::task::JoinHandle<()>>,
    /// Channel receiver for streaming events
    rx: Option<StreamingReceiver>,
    /// Buffered streaming content
    buffer: Option<StreamingBuffer>,
    /// Whether we're currently streaming
    is_streaming: bool,
    /// Streaming parser for markdown parsing
    parser: Option<StreamingParser>,
}

impl StreamingManager {
    pub fn new() -> Self {
        Self {
            task: None,
            rx: None,
            buffer: None,
            is_streaming: false,
            parser: None,
        }
    }

    // =========================================================================
    // Task and channel methods
    // =========================================================================

    /// Set the streaming task and receiver channel
    ///
    /// Called when starting a new LLM streaming request.
    pub fn set_task_and_receiver(
        &mut self,
        task: tokio::task::JoinHandle<()>,
        rx: StreamingReceiver,
    ) {
        self.task = Some(task);
        self.rx = Some(rx);
    }

    /// Get mutable reference to the receiver for polling
    pub fn rx_mut(&mut self) -> Option<&mut StreamingReceiver> {
        self.rx.as_mut()
    }

    /// Take the task handle (for checking completion)
    pub fn take_task(&mut self) -> Option<tokio::task::JoinHandle<()>> {
        self.task.take()
    }

    /// Check if task is finished
    pub fn is_task_finished(&self) -> bool {
        self.task.as_ref().is_some_and(|t| t.is_finished())
    }

    /// Clear task and receiver (called when streaming completes)
    pub fn clear_task_and_receiver(&mut self) {
        self.task = None;
        self.rx = None;
    }

    /// Check if we have an active receiver
    pub fn has_receiver(&self) -> bool {
        self.rx.is_some()
    }

    // =========================================================================
    // Buffer management
    // =========================================================================

    /// Start streaming with a new buffer
    pub fn start_streaming(&mut self, buffer: StreamingBuffer) {
        self.buffer = Some(buffer);
        self.is_streaming = true;
    }

    /// Start streaming with parser
    pub fn start_streaming_with_parser(&mut self, buffer: StreamingBuffer) {
        self.start_streaming(buffer);
        self.parser = Some(StreamingParser::new());
    }

    /// Stop streaming and return the buffer
    ///
    /// Note: This does NOT clear task/receiver - call `clear_task_and_receiver()`
    /// separately when the streaming task completes.
    pub fn stop_streaming(&mut self) -> Option<StreamingBuffer> {
        self.is_streaming = false;
        self.parser = None; // Clear parser when stopping
        self.buffer.take()
    }

    /// Check if currently streaming
    pub fn is_streaming(&self) -> bool {
        self.is_streaming
    }

    /// Get mutable reference to buffer
    pub fn buffer_mut(&mut self) -> Option<&mut StreamingBuffer> {
        self.buffer.as_mut()
    }

    /// Get reference to buffer
    pub fn buffer(&self) -> Option<&StreamingBuffer> {
        self.buffer.as_ref()
    }

    /// Append content to the streaming buffer
    pub fn append(&mut self, delta: &str) -> Option<String> {
        if let Some(buf) = &mut self.buffer {
            buf.append(delta)
        } else {
            None
        }
    }

    /// Finalize streaming and return all content
    pub fn finalize(&mut self) -> String {
        if let Some(buf) = &mut self.buffer {
            buf.finalize()
        } else {
            String::new()
        }
    }

    /// Get all content from buffer
    pub fn all_content(&self) -> String {
        if let Some(buf) = &self.buffer {
            buf.all_content().to_string()
        } else {
            String::new()
        }
    }

    // =========================================================================
    // Parser methods
    // =========================================================================

    /// Get mutable reference to parser
    pub fn parser_mut(&mut self) -> Option<&mut StreamingParser> {
        self.parser.as_mut()
    }

    /// Get reference to parser
    pub fn parser(&self) -> Option<&StreamingParser> {
        self.parser.as_ref()
    }

    /// Clear the parser
    pub fn clear_parser(&mut self) {
        self.parser = None;
    }

    /// Check if parser exists
    pub fn has_parser(&self) -> bool {
        self.parser.is_some()
    }
}

impl Default for StreamingManager {
    fn default() -> Self {
        Self::new()
    }
}
