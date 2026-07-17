//! Topic-based session title generation.
//!
//! One-shot LLM completion over the opening exchange of a session, using
//! the session's own configured provider. No tools, no history mutation —
//! deliberately outside the agent turn machinery. The genai-level call
//! itself lives behind the provider seam (`provider::title`).

use super::*;
use crucible_core::turn::NodeContent;

/// Removes the session from the in-flight set when generation finishes,
/// whatever the exit path.
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
    /// Generate and persist a topic-based title for a session.
    ///
    /// Idempotent: returns the existing title when one is already set (the
    /// RPC path and the `message_complete` auto-trigger can both fire).
    /// Falls back to a truncation of the first user message when no chat
    /// client can be built or the LLM call fails, so a session never stays
    /// untitled once it has content.
    pub async fn generate_session_title(
        &self,
        session_id: &str,
        event_tx: &broadcast::Sender<SessionEventMessage>,
    ) -> Result<String, AgentError> {
        let session = self
            .session_manager
            .get_session(session_id)
            .ok_or_else(|| AgentError::SessionNotFound(session_id.to_string()))?;
        if let Some(existing) = session.title.as_deref() {
            if !existing.trim().is_empty() {
                return Ok(existing.to_string());
            }
        }

        if self
            .titles_in_flight
            .insert(session_id.to_string(), ())
            .is_some()
        {
            return Err(AgentError::ConcurrentRequest(session_id.to_string()));
        }
        let _guard = InFlightGuard {
            map: self.titles_in_flight.clone(),
            key: session_id.to_string(),
        };

        let tree = self
            .get_or_rebuild_session_tree(session_id, &session.jsonl_path())
            .await;
        let (first_user, first_agent) = {
            let tree = tree.lock().await;
            let mut user = None;
            let mut agent = None;
            for (_, node) in tree.iter() {
                match &node.content {
                    NodeContent::User { text } if user.is_none() => user = Some(text.clone()),
                    NodeContent::Agent { text } if user.is_some() && agent.is_none() => {
                        agent = Some(text.clone())
                    }
                    _ => {}
                }
                if user.is_some() && agent.is_some() {
                    break;
                }
            }
            (user, agent)
        };
        let Some(first_user) = first_user else {
            return Err(AgentError::NotSupported(format!(
                "session {session_id} has no user message to derive a title from"
            )));
        };

        let title = match &session.agent {
            Some(agent_config) => {
                let lua_handle: Option<Lua> = match &self.plugin_loader {
                    Some(loader) => {
                        let guard = loader.lock().await;
                        guard.as_ref().map(|l| l.executor().lua().clone())
                    }
                    None => None,
                };
                match crate::agent_factory::build_chat_client_for_agent(
                    agent_config,
                    lua_handle.as_ref(),
                ) {
                    Ok((client, model_iden)) => {
                        match crate::provider::title::generate_title_via_backend(
                            &client,
                            &model_iden,
                            &first_user,
                            first_agent.as_deref(),
                        )
                        .await
                        {
                            Ok(t) if !t.is_empty() => t,
                            Ok(_) => truncate_to_title(&first_user),
                            Err(e) => {
                                debug!(
                                    session_id = %session_id,
                                    error = %e,
                                    "LLM title generation failed; falling back to truncation"
                                );
                                truncate_to_title(&first_user)
                            }
                        }
                    }
                    Err(e) => {
                        debug!(
                            session_id = %session_id,
                            error = %e,
                            "No usable chat client for title generation; falling back to truncation"
                        );
                        truncate_to_title(&first_user)
                    }
                }
            }
            None => truncate_to_title(&first_user),
        };

        self.session_manager
            .set_title(session_id, title.clone())
            .await?;
        emit_event(
            event_tx,
            SessionEventMessage::new(
                session_id,
                "title_changed",
                serde_json::json!({ "title": title }),
            ),
        );
        info!(session_id = %session_id, title = %title, "Session title generated");
        Ok(title)
    }
}

/// Fallback: a concise title by smart truncation of the first user message.
/// Char-boundary safe for multi-byte UTF-8 (CJK, emoji). Also used by the
/// startup catch-up sweep, which titles persisted sessions without an LLM.
pub(crate) fn truncate_to_title(message: &str) -> String {
    const MAX_LEN: usize = 60;

    let cleaned: String = message.split_whitespace().collect::<Vec<_>>().join(" ");

    if cleaned.chars().count() <= MAX_LEN {
        return cleaned;
    }

    let truncated: String = cleaned.chars().take(MAX_LEN).collect();
    if let Some(last_space) = truncated.rfind(' ') {
        if last_space > MAX_LEN / 2 {
            return format!("{}...", &truncated[..last_space]);
        }
    }

    format!("{truncated}...")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_passes_short_messages_through() {
        assert_eq!(truncate_to_title("  fix   the bug  "), "fix the bug");
    }

    #[test]
    fn truncate_breaks_at_word_boundary() {
        let msg =
            "please help me refactor the session manager so that archived sessions stay hidden";
        let title = truncate_to_title(msg);
        assert!(title.len() <= 64);
        assert!(title.ends_with("..."));
        assert!(!title.contains("hidden"));
    }

    #[test]
    fn truncate_is_utf8_safe() {
        let msg = "日本語のテキスト".repeat(20);
        let title = truncate_to_title(&msg);
        assert!(title.ends_with("..."));
        assert_eq!(title.chars().count(), 63);
    }
}
