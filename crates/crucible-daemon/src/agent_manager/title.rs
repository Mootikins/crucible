//! Core session titling, driven by completed turns and shared by every client.
//! Lua supplies only prompt formatting and sanitization; generation and writes
//! belong to this owner, independently of installed plugins.

use super::*;
use crucible_core::protocol::session_events::{SessionEventPayload, TurnPayload};
use crucible_core::transcript::{ItemBody, TranscriptFold, TranscriptOp};
use crucible_core::turn::TurnStatus;
use mlua::LuaSerdeExt;

struct InFlightGuard {
    map: Arc<DashMap<String, ()>>,
    key: String,
}
impl Drop for InFlightGuard {
    fn drop(&mut self) {
        self.map.remove(&self.key);
    }
}

impl AgentManager {
    /// Explicit regeneration. Compare against the observed title at commit,
    /// so a manual rename while the model is answering wins.
    pub async fn generate_session_title(
        &self,
        session_id: &str,
        event_tx: &crate::EventBus,
    ) -> Result<String, AgentError> {
        let session = self
            .session_manager
            .get_session(session_id)
            .ok_or_else(|| AgentError::SessionNotFound(session_id.into()))?;
        let lines = self
            .session_manager
            .load_session_events(&session.id, None, None)
            .await?;
        let events = crate::observe::stored_events(session_id, lines);
        let ids: Vec<_> = TranscriptFold::of_events(&events)
            .items
            .iter()
            .filter_map(|item| match &item.body {
                ItemBody::UserTurn { origin, .. }
                    if origin.as_ref().is_none_or(|o| o.plugin().is_none()) =>
                {
                    Some(item.id.clone())
                }
                _ => None,
            })
            .collect();
        let ids = &ids[ids.len().saturating_sub(3)..];
        let before = events.last().and_then(|event| event.seq);
        let untitled = session
            .title
            .as_deref()
            .is_none_or(|title| title.trim().is_empty());
        let result = self
            .title_from_exchanges(
                session_id,
                session.title,
                exchanges_for(&events, ids),
                None,
                event_tx,
            )
            .await
            .map(Option::unwrap_or_default);
        if result.is_err() && untitled {
            // Explicit generation owns the same guard. If it failed while a
            // successful turn was deferred, hand that boundary to core policy.
            if let Ok(lines) = self
                .session_manager
                .load_session_events(&session.id, None, None)
                .await
            {
                let events = crate::observe::stored_events(session_id, lines);
                if let Some((_, seq)) = successful_turns(&events).last() {
                    if *seq > before {
                        if let Some(event) = events.iter().find(|event| event.seq == *seq) {
                            if let Ok(Some(title)) =
                                self.auto_title_after_turn(event, event_tx).await
                            {
                                return Ok(title);
                            }
                        }
                    }
                }
            }
        }
        result
    }

    /// One successful terminal event, after the journal has persisted it.
    /// Eligibility comes from history, so reloads and duplicate delivery do
    /// not create a second counter or inflate the number of real user turns.
    pub(crate) async fn auto_title_after_turn(
        &self,
        event: &SessionEventMessage,
        event_tx: &crate::EventBus,
    ) -> Result<Option<String>, AgentError> {
        if !matches!(
            event.payload(),
            Ok(SessionEventPayload::Turn(TurnPayload::TurnFinished {
                status: TurnStatus::Completed,
                ..
            }))
        ) {
            return Ok(None);
        }
        let threshold = super::configured::title_after_turns();
        if threshold == 0 {
            return Ok(None);
        }
        let mut boundary = event.seq;
        loop {
            let Some(session) = self.session_manager.get_session(&event.session_id) else {
                return Ok(None);
            };
            if session
                .title
                .as_deref()
                .is_some_and(|t| !t.trim().is_empty())
            {
                return Ok(None);
            }
            let lines = self
                .session_manager
                .load_session_events(&session.id, None, None)
                .await?;
            let events = crate::observe::stored_events(&event.session_id, lines);
            let turns = successful_turns(&events);
            if turns.len() < threshold || turns.last().and_then(|(_, seq)| *seq) != boundary {
                return Ok(None);
            }
            let result = self
                .title_from_exchanges(
                    &event.session_id,
                    session.title,
                    completed_exchanges(&events, threshold),
                    boundary,
                    event_tx,
                )
                .await;
            if result.is_ok() {
                return result;
            }
            // A later boundary may have arrived while this request held the
            // guard. Its watcher deferred to us; recover it after failure.
            let lines = self
                .session_manager
                .load_session_events(&session.id, None, None)
                .await?;
            let events = crate::observe::stored_events(&session.id, lines);
            let newest = successful_turns(&events).last().and_then(|(_, seq)| *seq);
            if newest <= boundary {
                return result;
            }
            boundary = newest;
        }
    }

    async fn title_from_exchanges(
        &self,
        session_id: &str,
        expected: Option<String>,
        exchanges: Vec<[String; 2]>,
        boundary: Option<u64>,
        event_tx: &crate::EventBus,
    ) -> Result<Option<String>, AgentError> {
        if exchanges.is_empty() {
            return Err(AgentError::NotSupported(format!(
                "session {session_id} has no user message to title"
            )));
        }
        match self.titles_in_flight.entry(session_id.into()) {
            dashmap::mapref::entry::Entry::Occupied(_) => {
                if boundary.is_some() {
                    return Ok(None);
                }
                return Err(AgentError::ConcurrentRequest(session_id.into()));
            }
            dashmap::mapref::entry::Entry::Vacant(entry) => {
                entry.insert(());
            }
        }
        let _guard = InFlightGuard {
            map: self.titles_in_flight.clone(),
            key: session_id.into(),
        };
        let current = self
            .session_manager
            .get_session(session_id)
            .ok_or_else(|| AgentError::SessionNotFound(session_id.into()))?;
        if current.title != expected {
            return Ok(current.title);
        }
        if let Some(seq) = boundary {
            if self
                .title_attempts
                .get(session_id)
                .is_some_and(|previous| *previous >= seq)
            {
                return Ok(None);
            }
            self.title_attempts.insert(session_id.into(), seq);
        }
        let lua = self
            .plugin_lua()
            .await
            .ok_or_else(|| AgentError::NotSupported("core Lua runtime is unavailable".into()))?;
        let module: mlua::Table = lua
            .load("return require('crucible.session_title')")
            .eval()
            .map_err(|e| AgentError::InvalidConfig(e.to_string()))?;
        let format: mlua::Function = module
            .get("format")
            .map_err(|e| AgentError::InvalidConfig(e.to_string()))?;
        let (system, prompt): (String, String) = format
            .call(
                lua.to_value(&exchanges)
                    .map_err(|e| AgentError::InvalidConfig(e.to_string()))?,
            )
            .map_err(|e| AgentError::InvalidConfig(e.to_string()))?;
        let answer = self
            .complete_once(
                session_id,
                super::completion::OneShotParams {
                    system: Some(system),
                    prompt,
                    timeout: None,
                },
            )
            .await?;
        let sanitize: mlua::Function = module
            .get("sanitize")
            .map_err(|e| AgentError::InvalidConfig(e.to_string()))?;
        let title: String = sanitize
            .call(answer)
            .map_err(|e| AgentError::InvalidConfig(e.to_string()))?;
        if title.is_empty() {
            return Err(AgentError::InvalidConfig(
                "title model answered with a blank title".into(),
            ));
        }
        // Saving, publishing and the compare happen under one session lock.
        // Failure leaves the title absent, allowing a later turn to retry.
        let committed = self
            .session_manager
            .commit_generated_title(session_id, title, expected, event_tx)
            .await?;
        Ok(committed)
    }
}

fn successful_turns(events: &[SessionEventMessage]) -> Vec<(String, Option<u64>)> {
    let mut open = None;
    let mut fold = TranscriptFold::new();
    let mut turns = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for event in events {
        let ops = fold.apply(event);
        match event.payload() {
            Ok(SessionEventPayload::Turn(TurnPayload::UserMessage { origin, .. })) => {
                open = if origin.as_ref().is_none_or(|o| o.plugin().is_none()) {
                    ops.iter().find_map(|op| match op {
                        TranscriptOp::Upsert { item, .. }
                            if matches!(item.body, ItemBody::UserTurn { .. }) =>
                        {
                            Some(item.id.clone())
                        }
                        _ => None,
                    })
                } else {
                    None
                };
            }
            Ok(SessionEventPayload::Turn(TurnPayload::TurnFinished { status, .. })) => {
                if let Some(id) = open.take() {
                    if status == TurnStatus::Completed && seen.insert(id.clone()) {
                        turns.push((id, event.seq));
                    }
                }
            }
            _ => {}
        }
    }
    turns
}

fn completed_exchanges(events: &[SessionEventMessage], limit: usize) -> Vec<[String; 2]> {
    let ids: Vec<_> = successful_turns(events)
        .into_iter()
        .take(limit)
        .map(|(id, _)| id)
        .collect();
    exchanges_for(events, &ids)
}

fn exchanges_for(events: &[SessionEventMessage], ids: &[String]) -> Vec<[String; 2]> {
    let transcript = TranscriptFold::of_events(events);
    ids.iter()
        .filter_map(|id| {
            let user = transcript.items.iter().find_map(|item| match &item.body {
                ItemBody::UserTurn { content, .. } if item.id == *id => Some(content.clone()),
                _ => None,
            })?;
            let assistant = transcript
                .items
                .iter()
                .filter_map(|item| match &item.body {
                    ItemBody::AssistantSegment { text, .. }
                        if item.turn_id.as_ref() == Some(id) && !text.is_empty() =>
                    {
                        Some(text.as_str())
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n\n");
            Some([user, assistant])
        })
        .collect()
}

#[cfg(test)]
mod selection_tests {
    use super::*;
    fn event(name: &str, data: serde_json::Value) -> SessionEventMessage {
        SessionEventMessage::new("s", name, data)
    }
    fn turn(id: &str, origin: serde_json::Value, status: &str) -> Vec<SessionEventMessage> {
        vec![
            event(
                "user_message",
                serde_json::json!({"message_id":id,"content":id,"origin":origin}),
            ),
            event(
                "message_complete",
                serde_json::json!({"message_id":id,"full_response":format!("answer {id}")}),
            ),
            event("turn_finished", serde_json::json!({"status":status})),
        ]
    }
    #[test]
    fn title_selector_counts_only_successful_real_turns_and_ignores_duplicate_terminal_events() {
        let mut events = turn("one", serde_json::Value::Null, "completed");
        events.push(event(
            "turn_finished",
            serde_json::json!({"status":"completed"}),
        ));
        events.extend(turn("cancelled", serde_json::Value::Null, "cancelled"));
        events.extend(turn("failed", serde_json::Value::Null, "failed"));
        events.extend(turn(
            "plugin",
            serde_json::json!({"kind":"plugin","name":"reflection"}),
            "completed",
        ));
        events.push(event(
            "context_injected",
            serde_json::json!({"role":"user","content":"injected"}),
        ));
        events.extend(turn(
            "relay",
            serde_json::json!({"kind":"relay","name":"discord"}),
            "completed",
        ));
        events.extend(turn("three", serde_json::Value::Null, "completed"));
        events.extend(turn("four", serde_json::Value::Null, "completed"));
        assert_eq!(
            completed_exchanges(&events, 3),
            vec![
                ["one".to_string(), "answer one".to_string()],
                ["relay".to_string(), "answer relay".to_string()],
                ["three".to_string(), "answer three".to_string()],
            ]
        );
    }
    #[test]
    fn title_selector_uses_canonical_ids_for_legacy_openings() {
        let events = vec![
            event(
                "user_message",
                serde_json::json!({"content":"legacy question"}),
            ),
            event(
                "message_complete",
                serde_json::json!({"full_response":"legacy answer"}),
            ),
            event("turn_finished", serde_json::json!({"status":"completed"})),
        ];
        assert_eq!(
            completed_exchanges(&events, 1),
            vec![["legacy question".to_string(), "legacy answer".to_string()]]
        );
    }
}
