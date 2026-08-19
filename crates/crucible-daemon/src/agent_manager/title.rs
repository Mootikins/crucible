//! Session titling: when a session gets a title, and what happens when
//! nothing answers.
//!
//! *How* a title is asked for is not here and is not in Rust at all. A plugin
//! publishes itself on the `session_title` channel and names a command; the
//! bundled one is `runtime/plugins/auto-title/`, which owns the prompt, the
//! clip and the sanitizer that used to live in `provider/title.rs`. What stays
//! daemon-side is the part every client depends on being uniform: when titling
//! fires, that it fires once, that the title is persisted, that `title_changed`
//! is emitted, and that a session with content never stays untitled.

use super::*;
use crucible_core::turn::NodeContent;

/// The publication channel a session-title provider declares itself on.
///
/// The daemon looks up the CHANNEL, never a plugin name, so a user plugin
/// publishing the same key replaces the bundled behaviour outright.
const TITLE_CHANNEL: &str = "session_title";

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
    /// Falls back to a truncation of the first user message whenever the
    /// titling plugin does not answer — including when there is no plugin at
    /// all — so a session never stays untitled once it has content.
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
            .get_or_rebuild_session_tree(
                session_id,
                &session.jsonl_path(self.session_manager.sessions_root()),
            )
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

        let title = match self
            .plugin_title(session_id, &first_user, first_agent.as_deref())
            .await
        {
            Some(title) => title,
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

    /// Ask the plugin that publishes [`TITLE_CHANNEL`] for a title.
    ///
    /// `None` on every failure — no plugin, no command, a raise, an empty
    /// answer — because the caller's fallback is the honest answer to all of
    /// them, and a session with content must never stay untitled. The reason
    /// is logged at debug, where the rest of the titling path already logs.
    async fn plugin_title(
        &self,
        session_id: &str,
        user: &str,
        assistant: Option<&str>,
    ) -> Option<String> {
        // Explicit, not `?`: a manager wired without a plugin loader (tests,
        // the startup sweep before plugins land) is the one path here that
        // used to fall back silently while every sibling said why.
        let Some(publications) = self.publications().await else {
            debug!(session_id = %session_id, "No plugin runtime; truncating instead");
            return None;
        };
        let mut providers = publications.get(TITLE_CHANNEL);
        if providers.is_empty() {
            debug!(session_id = %session_id, "No plugin titles sessions; truncating instead");
            return None;
        }
        if providers.len() > 1 {
            // Sorted by plugin name, so the winner is at least stable across
            // restarts. Named rather than silently resolved: two titlers is a
            // configuration the operator should know about.
            warn!(
                titlers = ?providers.iter().map(|(p, _)| p.as_str()).collect::<Vec<_>>(),
                "More than one plugin titles sessions; using the first by name"
            );
        }
        let (plugin, declaration) = providers.remove(0);

        let command = declaration.get("command").and_then(|v| v.as_str());
        let Some(command) = command else {
            warn!(plugin = %plugin, "Plugin publishes '{TITLE_CHANNEL}' but names no command");
            return None;
        };

        // An absent assistant turn is an ABSENT key, not a null: mlua renders
        // a JSON null as a lightuserdata, which is truthy in Lua, so
        // `if args.assistant then` would be true for a session that has none.
        let mut args = serde_json::json!({ "session_id": session_id, "user": user });
        if let Some(assistant) = assistant {
            args["assistant"] = serde_json::Value::String(assistant.to_string());
        }
        let Some(registry) = self.plugin_registry().await else {
            debug!(
                session_id = %session_id,
                plugin = %plugin,
                "No plugin registry to call the title command on; truncating instead"
            );
            return None;
        };
        let answer = match registry.run_command(command, args).await {
            Ok(Some(answer)) => answer,
            Ok(None) => {
                warn!(
                    plugin = %plugin,
                    command = %command,
                    "Plugin names a title command it does not declare"
                );
                return None;
            }
            Err(e) => {
                debug!(
                    session_id = %session_id,
                    plugin = %plugin,
                    error = %e,
                    "Plugin title generation failed; falling back to truncation"
                );
                return None;
            }
        };

        title_from_result(&answer)
    }
}

/// The title a provider's command answered with.
///
/// Accepts `{ title = "…" }` or a bare string, and treats blank as absent: a
/// model answering with whitespace is the case the sanitizer cannot rescue,
/// and an empty title is worse than a truncated one.
fn title_from_result(value: &serde_json::Value) -> Option<String> {
    let raw = value
        .get("title")
        .and_then(|v| v.as_str())
        .or_else(|| value.as_str())?;
    let trimmed = raw.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
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
    fn a_title_is_read_from_either_shape() {
        let object = serde_json::json!({ "title": "Fixing the auth flow" });
        let bare = serde_json::json!("Fixing the auth flow");
        let expected = Some("Fixing the auth flow".to_string());
        assert_eq!(title_from_result(&object), expected);
        assert_eq!(title_from_result(&bare), expected);
    }

    /// Blank is absent: the caller then truncates, which is a real title.
    #[test]
    fn a_blank_or_absent_title_is_no_title() {
        assert_eq!(title_from_result(&serde_json::json!({})), None);
        assert_eq!(
            title_from_result(&serde_json::json!({ "title": "  " })),
            None
        );
        assert_eq!(title_from_result(&serde_json::json!(null)), None);
    }

    #[test]
    fn a_title_is_trimmed() {
        assert_eq!(
            title_from_result(&serde_json::json!({ "title": "  Session sweep\n" })),
            Some("Session sweep".to_string())
        );
    }

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
