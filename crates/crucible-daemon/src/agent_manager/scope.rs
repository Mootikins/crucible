//! Mid-session scope mutations: connected kilns and workspace.
//!
//! Detach is always safe (it only shrinks future retrieval/tool scope);
//! attach-side trust validation lives in the RPC handlers, which have the
//! LLM config. All mutations are rejected mid-turn and invalidate BOTH the
//! agent handle and the tool dispatcher — each bakes in workspace/kiln
//! state at build time (system prompt, WorkspaceTools, kiln MCP tools),
//! while precognition/search already read the session fresh every turn.

use super::*;
use crate::event_emitter::emit_event;
use crucible_core::Session;
use std::path::{Path, PathBuf};

impl AgentManager {
    fn ensure_idle(&self, session_id: &str) -> Result<(), AgentError> {
        if self.request_state.contains_key(session_id) {
            return Err(AgentError::ConcurrentRequest(session_id.to_string()));
        }
        Ok(())
    }

    /// Evict everything that baked the old scope in at build time.
    fn invalidate_scope_caches(&self, session_id: &str) {
        self.agent_cache.remove(session_id);
        self.session_dispatchers.remove(session_id);
    }

    fn emit_scope_changed(
        &self,
        event_tx: Option<&broadcast::Sender<SessionEventMessage>>,
        session: &Session,
    ) {
        if let Some(tx) = event_tx {
            let data = serde_json::json!({
                "kiln": session.kiln,
                "workspace": session.workspace,
                "connected_kilns": session.connected_kilns,
            });
            if !emit_event(
                tx,
                SessionEventMessage::new(&session.id, "scope_changed", data),
            ) {
                tracing::debug!("Failed to emit scope_changed event (no subscribers)");
            }
        }
    }

    /// Attach a kiln to the session's connected set. Idempotent; the primary
    /// kiln cannot be attached twice.
    pub async fn connect_kiln(
        &self,
        session_id: &str,
        kiln: &Path,
        event_tx: Option<&broadcast::Sender<SessionEventMessage>>,
    ) -> Result<Session, AgentError> {
        self.ensure_idle(session_id)?;
        let mut session = self
            .session_manager
            .get_session(session_id)
            .ok_or_else(|| AgentError::SessionNotFound(session_id.to_string()))?;

        let kiln = kiln.to_path_buf();
        if session.kiln == kiln {
            return Err(AgentError::InvalidConfig(
                "kiln is already the session's primary kiln".to_string(),
            ));
        }
        if session.connected_kilns.contains(&kiln) {
            return Ok(session);
        }

        session.connected_kilns.push(kiln);
        self.session_manager
            .update_session(&session)
            .await
            .map_err(AgentError::Session)?;
        self.invalidate_scope_caches(session_id);
        self.emit_scope_changed(event_tx, &session);
        Ok(session)
    }

    /// Detach a connected kiln. The primary kiln cannot be detached — the
    /// session itself is stored there.
    pub async fn disconnect_kiln(
        &self,
        session_id: &str,
        kiln: &Path,
        event_tx: Option<&broadcast::Sender<SessionEventMessage>>,
    ) -> Result<Session, AgentError> {
        self.ensure_idle(session_id)?;
        let mut session = self
            .session_manager
            .get_session(session_id)
            .ok_or_else(|| AgentError::SessionNotFound(session_id.to_string()))?;

        if session.kiln == kiln {
            return Err(AgentError::InvalidConfig(
                "cannot detach the session's primary kiln — the session is stored there"
                    .to_string(),
            ));
        }
        let before = session.connected_kilns.len();
        session.connected_kilns.retain(|k| k != kiln);
        if session.connected_kilns.len() == before {
            return Ok(session);
        }

        self.session_manager
            .update_session(&session)
            .await
            .map_err(AgentError::Session)?;
        self.invalidate_scope_caches(session_id);
        self.emit_scope_changed(event_tx, &session);
        Ok(session)
    }

    /// Set or clear the session's workspace. `None` detaches: the workspace
    /// falls back to the kiln path (the same state a workspace-less create
    /// produces — see `Session::new`). Rejected for ACP sessions, whose
    /// external agent process runs in the workspace it was spawned with.
    pub async fn set_workspace(
        &self,
        session_id: &str,
        workspace: Option<PathBuf>,
        event_tx: Option<&broadcast::Sender<SessionEventMessage>>,
    ) -> Result<Session, AgentError> {
        self.ensure_idle(session_id)?;
        let mut session = self
            .session_manager
            .get_session(session_id)
            .ok_or_else(|| AgentError::SessionNotFound(session_id.to_string()))?;

        let is_acp = session
            .agent
            .as_ref()
            .map(|a| a.agent_type == "acp")
            .unwrap_or(false);
        if is_acp {
            return Err(AgentError::NotSupported(
                "ACP agents run in the workspace they were spawned with — start a new session to change it"
                    .to_string(),
            ));
        }

        let new_workspace = workspace.unwrap_or_else(|| session.kiln.clone());
        if session.workspace == new_workspace {
            return Ok(session);
        }

        session.workspace = new_workspace;
        self.session_manager
            .update_session(&session)
            .await
            .map_err(AgentError::Session)?;
        self.invalidate_scope_caches(session_id);
        self.emit_scope_changed(event_tx, &session);
        Ok(session)
    }
}

#[cfg(test)]
mod tests {
    // Scope-mutation behavior is covered end-to-end in
    // tests/rpc_session_scope_e2e.rs — the mutations need a real
    // SessionManager + storage, which the RPC test server provides.
}
