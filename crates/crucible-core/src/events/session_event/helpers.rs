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
        SessionEvent::AgentResponded { .. } => "agent:responded".into(),
        SessionEvent::AgentThinking { .. } => "agent:thinking".into(),
        SessionEvent::ToolCalled { name, .. } => name.clone(),
        SessionEvent::ToolCompleted { name, .. } => name.clone(),
        SessionEvent::SessionStarted { config, .. } => format!("session:{}", config.session_id),
        SessionEvent::SessionEnded { .. } => "session:ended".into(),
        SessionEvent::TextDelta { seq, .. } => format!("streaming:delta:{}", seq),
        SessionEvent::InteractionRequested {
            request_id,
            request,
        } => {
            format!("interaction:{}:{}", request.kind(), request_id)
        }
        SessionEvent::InteractionCompleted { request_id, .. } => {
            format!("interaction:completed:{}", request_id)
        }
        SessionEvent::DelegationSpawned { delegation_id, .. } => {
            format!("delegation:spawned:{}", delegation_id)
        }
        SessionEvent::DelegationCompleted { delegation_id, .. } => {
            format!("delegation:completed:{}", delegation_id)
        }
        SessionEvent::DelegationFailed { delegation_id, .. } => {
            format!("delegation:failed:{}", delegation_id)
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
        SessionEvent::AgentResponded { content, .. } => Some(content.clone()),
        SessionEvent::AgentThinking { thought } => Some(thought.clone()),
        SessionEvent::ToolCalled { args, .. } => Some(args.to_string()),
        SessionEvent::ToolCompleted { result, .. } => Some(result.clone()),
        SessionEvent::SessionStarted { .. } => None,
        SessionEvent::SessionEnded { reason } => Some(reason.clone()),
        SessionEvent::TextDelta { delta, .. } => Some(delta.clone()),
        SessionEvent::InteractionRequested { .. } => None,
        SessionEvent::InteractionCompleted { .. } => None,
        SessionEvent::DelegationSpawned { prompt, .. } => Some(prompt.clone()),
        SessionEvent::DelegationCompleted { result_summary, .. } => Some(result_summary.clone()),
        SessionEvent::DelegationFailed { error, .. } => Some(error.clone()),
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
        SessionEvent::AgentResponded { content, .. } => content.len(),
        SessionEvent::AgentThinking { thought } => thought.len(),
        SessionEvent::ToolCalled { args, .. } => args.to_string().len(),
        SessionEvent::ToolCompleted { result, error, .. } => {
            result.len() + error.as_ref().map(|e| e.len()).unwrap_or(0)
        }
        SessionEvent::SessionStarted { .. } => 100,
        SessionEvent::SessionEnded { reason } => reason.len(),
        SessionEvent::TextDelta { delta, .. } => delta.len(),
        SessionEvent::InteractionRequested { .. } => 100,
        SessionEvent::InteractionCompleted { .. } => 50,
        SessionEvent::DelegationSpawned { prompt, .. } => prompt.len(),
        SessionEvent::DelegationCompleted { result_summary, .. } => result_summary.len(),
        SessionEvent::DelegationFailed { error, .. } => error.len(),
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
