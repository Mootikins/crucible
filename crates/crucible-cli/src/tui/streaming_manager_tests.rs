//! Tests for StreamingManager
//!
//! Tests streaming lifecycle, buffer management, and parser state.
//! Note: Task/receiver methods require tokio runtime and are tested separately.

use super::streaming::StreamingBuffer;
use super::streaming_manager::StreamingManager;

#[test]
fn new_manager_not_streaming() {
    let mgr = StreamingManager::new();
    assert!(!mgr.is_streaming());
    assert!(!mgr.has_receiver());
    assert!(!mgr.has_parser());
}

#[test]
fn default_equals_new() {
    let default = StreamingManager::default();
    assert!(!default.is_streaming());
    assert!(!default.has_parser());
}

#[test]
fn buffer_is_none_initially() {
    let mgr = StreamingManager::new();
    assert!(mgr.buffer().is_none());
}

#[test]
fn start_streaming_sets_buffer_and_flag() {
    let mut mgr = StreamingManager::new();
    let buffer = StreamingBuffer::new();

    mgr.start_streaming(buffer);

    assert!(mgr.is_streaming());
    assert!(mgr.buffer().is_some());
}

#[test]
fn start_streaming_without_parser_has_no_parser() {
    let mut mgr = StreamingManager::new();
    mgr.start_streaming(StreamingBuffer::new());

    assert!(!mgr.has_parser());
    assert!(mgr.parser().is_none());
}

#[test]
fn start_streaming_with_parser_creates_parser() {
    let mut mgr = StreamingManager::new();
    let buffer = StreamingBuffer::new();

    mgr.start_streaming_with_parser(buffer);

    assert!(mgr.is_streaming());
    assert!(mgr.has_parser());
    assert!(mgr.parser().is_some());
}

#[test]
fn stop_streaming_returns_buffer() {
    let mut mgr = StreamingManager::new();
    mgr.start_streaming(StreamingBuffer::new());

    let returned = mgr.stop_streaming();

    assert!(returned.is_some());
    assert!(!mgr.is_streaming());
    assert!(mgr.buffer().is_none());
}

#[test]
fn stop_streaming_clears_parser() {
    let mut mgr = StreamingManager::new();
    mgr.start_streaming_with_parser(StreamingBuffer::new());
    assert!(mgr.has_parser());

    mgr.stop_streaming();

    assert!(!mgr.has_parser());
}

#[test]
fn stop_streaming_when_not_streaming_returns_none() {
    let mut mgr = StreamingManager::new();
    assert!(mgr.stop_streaming().is_none());
}

#[test]
fn append_to_buffer_accumulates_content() {
    let mut mgr = StreamingManager::new();
    mgr.start_streaming(StreamingBuffer::new());

    mgr.append("Hello ");
    mgr.append("World");

    assert_eq!(mgr.all_content(), "Hello World");
}

#[test]
fn append_when_not_streaming_returns_none() {
    let mut mgr = StreamingManager::new();
    assert!(mgr.append("text").is_none());
}

#[test]
fn finalize_returns_all_content() {
    let mut mgr = StreamingManager::new();
    mgr.start_streaming(StreamingBuffer::new());
    mgr.append("Complete message");

    let content = mgr.finalize();

    assert_eq!(content, "Complete message");
}

#[test]
fn finalize_when_not_streaming_returns_empty() {
    let mut mgr = StreamingManager::new();
    assert_eq!(mgr.finalize(), "");
}

#[test]
fn all_content_when_not_streaming_returns_empty() {
    let mgr = StreamingManager::new();
    assert_eq!(mgr.all_content(), "");
}

#[test]
fn buffer_mut_returns_some_when_streaming() {
    let mut mgr = StreamingManager::new();
    mgr.start_streaming(StreamingBuffer::new());

    assert!(mgr.buffer_mut().is_some());
}

#[test]
fn buffer_mut_returns_none_when_not_streaming() {
    let mut mgr = StreamingManager::new();
    assert!(mgr.buffer_mut().is_none());
}

#[test]
fn has_receiver_false_initially() {
    let mgr = StreamingManager::new();
    assert!(!mgr.has_receiver());
}

#[test]
fn clear_task_and_receiver_is_safe_when_empty() {
    let mut mgr = StreamingManager::new();
    mgr.clear_task_and_receiver();
    assert!(!mgr.has_receiver());
}

#[test]
fn rx_mut_returns_none_when_no_receiver() {
    let mut mgr = StreamingManager::new();
    assert!(mgr.rx_mut().is_none());
}

#[test]
fn take_task_returns_none_when_no_task() {
    let mut mgr = StreamingManager::new();
    assert!(mgr.take_task().is_none());
}

#[test]
fn is_task_finished_false_when_no_task() {
    let mgr = StreamingManager::new();
    assert!(!mgr.is_task_finished());
}

#[test]
fn clear_parser_removes_parser() {
    let mut mgr = StreamingManager::new();
    mgr.start_streaming_with_parser(StreamingBuffer::new());
    assert!(mgr.has_parser());

    mgr.clear_parser();

    assert!(!mgr.has_parser());
}

#[test]
fn clear_parser_safe_when_no_parser() {
    let mut mgr = StreamingManager::new();
    mgr.clear_parser();
    assert!(!mgr.has_parser());
}

#[test]
fn parser_mut_returns_some_when_parser_exists() {
    let mut mgr = StreamingManager::new();
    mgr.start_streaming_with_parser(StreamingBuffer::new());

    assert!(mgr.parser_mut().is_some());
}

#[test]
fn parser_mut_returns_none_when_no_parser() {
    let mut mgr = StreamingManager::new();
    mgr.start_streaming(StreamingBuffer::new());

    assert!(mgr.parser_mut().is_none());
}

#[test]
fn multiple_start_stop_cycles() {
    let mut mgr = StreamingManager::new();

    mgr.start_streaming_with_parser(StreamingBuffer::new());
    mgr.append("First");
    let first = mgr.stop_streaming();
    assert!(first.is_some());

    mgr.start_streaming(StreamingBuffer::new());
    mgr.append("Second");
    let second = mgr.stop_streaming();
    assert!(second.is_some());

    assert!(!mgr.is_streaming());
    assert!(!mgr.has_parser());
}

#[test]
fn append_unicode_content() {
    let mut mgr = StreamingManager::new();
    mgr.start_streaming(StreamingBuffer::new());

    mgr.append("Hello 世界 ");
    mgr.append("🚀 rocket");

    assert_eq!(mgr.all_content(), "Hello 世界 🚀 rocket");
}

#[test]
fn append_multiline_content() {
    let mut mgr = StreamingManager::new();
    mgr.start_streaming(StreamingBuffer::new());

    mgr.append("Line 1\n");
    mgr.append("Line 2\n");
    mgr.append("Line 3");

    assert_eq!(mgr.all_content(), "Line 1\nLine 2\nLine 3");
}

#[test]
fn append_empty_string() {
    let mut mgr = StreamingManager::new();
    mgr.start_streaming(StreamingBuffer::new());

    mgr.append("start");
    mgr.append("");
    mgr.append("end");

    assert_eq!(mgr.all_content(), "startend");
}

#[test]
fn streaming_state_independent_of_task_receiver() {
    let mut mgr = StreamingManager::new();

    mgr.start_streaming(StreamingBuffer::new());
    assert!(mgr.is_streaming());
    assert!(!mgr.has_receiver());

    mgr.clear_task_and_receiver();
    assert!(mgr.is_streaming());

    mgr.stop_streaming();
    assert!(!mgr.is_streaming());
}
