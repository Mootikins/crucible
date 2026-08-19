//! One-shot completions against a session's own client.
//!
//! The daemon half of `cru.sessions.complete`. It resolves the session's
//! configured provider the same way a turn would, then runs exactly one
//! exchange through [`crate::provider::oneshot`] — no tools, no history, no
//! writes back to the session.
//!
//! Deliberately outside the agent turn machinery, for the reason session
//! titling always was: this is a background question about a session, not a
//! turn in it. What is new is that the question itself now comes from Lua.

use super::*;
use serde::Deserialize;

/// A plugin's `cru.sessions.complete(session_id, opts)` options.
///
/// `timeout` is seconds; omitted means [`crate::provider::oneshot::DEFAULT_TIMEOUT_SECS`].
#[derive(Debug, Deserialize)]
pub(crate) struct OneShotParams {
    /// The user turn. The only required field.
    pub prompt: String,
    /// Optional system turn.
    #[serde(default)]
    pub system: Option<String>,
    /// Seconds to wait. `None` and `0` both mean the default.
    #[serde(default)]
    pub timeout: Option<u64>,
}

impl AgentManager {
    /// One completion against `session_id`'s resolved client, answered as text.
    ///
    /// Fails rather than falling back: a caller that wants a fallback owns it
    /// (the title path truncates the first user message), and a second
    /// fallback here would make which one you got depend on where the failure
    /// happened.
    pub(crate) async fn complete_once(
        &self,
        session_id: &str,
        params: OneShotParams,
    ) -> Result<String, AgentError> {
        let session = self
            .session_manager
            .get_session(session_id)
            .ok_or_else(|| AgentError::SessionNotFound(session_id.to_string()))?;
        let agent_config = session
            .agent
            .ok_or_else(|| AgentError::NoAgentConfigured(session_id.to_string()))?;

        // Off the startup-bound `OnceLock`, not the loader mutex — see
        // `plugin_lua`. Every caller of this is a background nicety and must
        // never queue behind a session start's plugin hooks.
        let lua_handle: Option<Lua> = self.plugin_lua().await;
        let (client, model) =
            crate::agent_factory::build_chat_client_for_agent(&agent_config, lua_handle.as_ref())?;

        crate::provider::oneshot::complete(
            &client,
            &model,
            params.system.as_deref(),
            &params.prompt,
            crate::provider::oneshot::timeout_from(params.timeout),
        )
        .await
        .map_err(AgentError::InvalidConfig)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Lua binding hands the options table straight through, so options
    /// naming only a prompt have to be enough.
    #[test]
    fn a_prompt_alone_is_valid_options() {
        let params: OneShotParams = serde_json::from_value(serde_json::json!({
            "prompt": "name this",
        }))
        .expect("prompt-only options");
        assert_eq!(params.prompt, "name this");
        assert!(params.system.is_none());
        assert!(params.timeout.is_none());
    }

    #[test]
    fn options_without_a_prompt_are_refused() {
        let parsed: Result<OneShotParams, _> =
            serde_json::from_value(serde_json::json!({ "system": "you name things" }));
        assert!(parsed.is_err(), "a completion with no prompt is not a call");
    }
}
