//! Streaming subsystem manager
//!
//! Manages streaming content from LLM providers, including:
//! - Receiving streaming events from agent
//! - Parsing markdown chunks into structured blocks
//! - Managing pending chunks awaiting processing

use crate::tui::streaming::StreamingBuffer;
use crate::tui::StreamBlock;
use tokio::sync::mpsc::Receiver;

// Import StreamingEvent if it exists
// use crate::tui::streaming_channel::StreamingEvent;

/// Manages streaming content from LLM
pub struct StreamingManager {
    /// Receives streaming events from agent
    // rx: Option<Receiver<StreamingEvent>>,
    /// Buffered streaming content
    buffer: Option<StreamingBuffer>,
    /// Pending chunks awaiting processing
    pending_chunks: Vec<String>,
    /// Whether we're currently streaming
    is_streaming: bool,
}

impl StreamingManager {
    pub fn new() -> Self {
        Self {
            // rx: None,
            buffer: None,
            pending_chunks: Vec::new(),
            is_streaming: false,
        }
    }

    /// Start streaming with a new buffer
    pub fn start_streaming(&mut self, buffer: StreamingBuffer) {
        self.buffer = Some(buffer);
        self.is_streaming = true;
        self.pending_chunks.clear();
    }

    /// Stop streaming and return the buffer
    pub fn stop_streaming(&mut self) -> Option<StreamingBuffer> {
        self.is_streaming = false;
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
}

impl Default for StreamingManager {
    fn default() -> Self {
        Self::new()
    }
}
