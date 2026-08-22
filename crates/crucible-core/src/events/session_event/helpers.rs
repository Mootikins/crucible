//! Helper functions for session events
//!
//! Utility functions for working with session events.

use super::SessionEvent;

/// Compute the identifier string for a session event.
///
/// Used for glob pattern matching against event handlers.
pub(super) fn identifier_for_event(event: &SessionEvent) -> String {
    match event {
        SessionEvent::MessageReceived { participant_id, .. } => {
            format!("message:{}", participant_id)
        }
        SessionEvent::InteractionRequested {
            request_id,
            request,
        } => {
            format!("interaction:{}:{}", request.kind(), request_id)
        }
        SessionEvent::Custom { name, .. } => name.clone(),
        SessionEvent::Internal(inner) => inner.identifier(),
    }
}

/// Extract the raw payload content from a session event.
///
/// Returns the main content or data associated with the event before truncation.
pub(super) fn payload_for_event(event: &SessionEvent) -> Option<String> {
    match event {
        SessionEvent::MessageReceived { content, .. } => Some(content.clone()),
        SessionEvent::InteractionRequested { .. } => None,
        SessionEvent::Custom { payload, .. } => Some(payload.to_string()),
        SessionEvent::Internal(inner) => inner.payload_content(),
    }
}

/// Estimate the content length for token estimation.
///
/// Returns a character count representing the meaningful content size of the event.
pub(super) fn estimate_content_len(event: &SessionEvent) -> usize {
    match event {
        SessionEvent::MessageReceived { content, .. } => content.len(),
        SessionEvent::InteractionRequested { .. } => 100,
        SessionEvent::Custom { payload, .. } => payload.to_string().len(),
        SessionEvent::Internal(inner) => inner.estimate_content_len(),
    }
}

/// Truncate a string to `max_len`, respecting UTF-8 char boundaries.
///
/// If the string is longer than `max_len`, it will be truncated at the nearest
/// valid UTF-8 character boundary.
pub(super) fn truncate(s: &str, max_len: usize) -> &str {
    if s.len() <= max_len {
        s
    } else {
        // Find a char boundary near max_len
        let mut end = max_len;
        while !s.is_char_boundary(end) && end > 0 {
            end -= 1;
        }
        &s[..end]
    }
}
