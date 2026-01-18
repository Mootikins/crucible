//! Simple passthrough reactor for sessions.

use std::collections::HashSet;

use async_trait::async_trait;

use crate::event_bus::EventContext;
use crate::reactor::{Reactor, ReactorMetadata, ReactorResult, SessionEvent};

pub struct SimpleReactor;

impl SimpleReactor {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SimpleReactor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Reactor for SimpleReactor {
    async fn handle_event(
        &self,
        _ctx: &mut EventContext,
        event: SessionEvent,
    ) -> ReactorResult<SessionEvent> {
        Ok(event)
    }

    async fn on_before_compact(&self, events: &[SessionEvent]) -> ReactorResult<String> {
        let mut messages = 0;
        let mut tool_calls = 0;
        let mut agent_responses = 0;
        let mut tools_used: HashSet<String> = HashSet::new();

        for event in events {
            match event {
                SessionEvent::MessageReceived { .. } => messages += 1,
                SessionEvent::ToolCalled { name, .. } => {
                    tool_calls += 1;
                    tools_used.insert(name.clone());
                }
                SessionEvent::AgentResponded { .. } => agent_responses += 1,
                _ => {}
            }
        }

        let tools_list: Vec<_> = tools_used.into_iter().collect();
        Ok(format!(
            "Session contained {} messages, {} tool calls, {} agent responses, {} total events. Tools used: {}",
            messages,
            tool_calls,
            agent_responses,
            events.len(),
            if tools_list.is_empty() { "none".to_string() } else { tools_list.join(", ") }
        ))
    }

    fn metadata(&self) -> ReactorMetadata {
        ReactorMetadata::new("SimpleReactor")
            .with_version("1.0.0")
            .with_description("Passthrough reactor that forwards events without processing")
    }
}

impl std::fmt::Debug for SimpleReactor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SimpleReactor").finish()
    }
}
