//! Mid-session scope mutations: the session's kiln set and workspace.
//!
//! Detach is always safe (it only shrinks future retrieval/tool scope);
//! attach-side trust validation lives in the RPC handlers, which have the
//! LLM config. Each mutation claims the session's `request_state` slot for
//! its whole duration via `RequestSlotGuard` — the same gate `send_message`
//! uses — so a mutation and a turn are mutually exclusive in both
//! directions: a send racing a mutation gets `ConcurrentRequest`, and a
//! mutation racing a send gets `ConcurrentRequest`. This closes the window
//! where a send could build and cache an agent from the pre-mutation
//! session *after* the caches were invalidated. On commit a mutation
//! invalidates BOTH the agent handle and the tool dispatcher — each bakes
//! in workspace/kiln state at build time (system prompt, WorkspaceTools,
//! kiln MCP tools), while precognition/search already read the session
//! fresh every turn.

use super::*;
use crate::event_emitter::emit_event;
use crucible_core::Session;
use std::path::{Path, PathBuf};

/// The filesystem containment for a session's workspace tools: `(allowed,
/// denied)`, deepest match wins.
///
/// Allowed is the session's kilns plus its own storage directory. Denied is
/// every place a *transcript* lives: the flat sessions root, and — because
/// migration is not guaranteed to have emptied them — each kiln's legacy
/// in-kiln `{kiln}/.crucible/sessions`. A kiln appearing mid-run is never
/// scanned and `migrate_one` deliberately skips on failure, so the legacy
/// subtree has to be denied rather than assumed gone.
///
/// Empty paths are filtered out on the way in. A kiln-less session with no
/// workspace carries `""` in both roles, and `""` as an allowed root is a
/// universal root at depth 0 — containment off, every denial out-ranked. An
/// empty kiln set degrades capabilities; it must never degrade containment.
pub(crate) fn session_containment_roots(
    session: &Session,
    sessions_root: &Path,
) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let allowed = session
        .kilns
        .iter()
        .cloned()
        .chain(std::iter::once(session.storage_path(sessions_root)))
        .filter(|root| !root.as_os_str().is_empty())
        .collect();
    let denied = std::iter::once(sessions_root.to_path_buf())
        .chain(
            session
                .kilns
                .iter()
                .map(|kiln| kiln.join(".crucible").join("sessions")),
        )
        .filter(|root| !root.as_os_str().is_empty())
        .collect();
    (allowed, denied)
}

impl AgentManager {
    /// Evict everything that baked the old scope in at build time.
    ///
    /// One lock over both, so this is atomic rather than two windows a racing
    /// build could land between — which is what the module doc above has been
    /// describing aspirationally.
    fn invalidate_scope_caches(&self, session_id: &str) {
        if let Some(slot) = self.existing_slot(session_id) {
            slot.invalidate_build();
        }
    }

    fn emit_scope_changed(
        &self,
        event_tx: Option<&broadcast::Sender<SessionEventMessage>>,
        session: &Session,
    ) {
        if let Some(tx) = event_tx {
            let data = serde_json::json!({
                "kilns": session.kilns,
                "workspace": session.workspace,
            });
            if !emit_event(
                tx,
                SessionEventMessage::new(&session.id, "scope_changed", data),
            ) {
                tracing::debug!("Failed to emit scope_changed event (no subscribers)");
            }
        }
    }

    /// Shared envelope for scope mutations: claim the request slot → load →
    /// apply the rule → (if changed) persist + invalidate caches + emit. The
    /// rule returns whether it changed anything; no-ops skip the write
    /// entirely. The slot claim is held for the whole body (guard dropped on
    /// return), so no `send_message` can interleave — a racing send hits the
    /// occupied slot and returns `ConcurrentRequest`, exactly as it would
    /// against another send.
    async fn mutate_scope(
        &self,
        session_id: &str,
        event_tx: Option<&broadcast::Sender<SessionEventMessage>>,
        apply: impl FnOnce(&mut Session) -> Result<bool, AgentError>,
    ) -> Result<Session, AgentError> {
        let _slot = RequestSlotGuard::acquire(self.request_state.clone(), session_id)?;
        let mut session = self
            .session_manager
            .get_session(session_id)
            .ok_or_else(|| AgentError::SessionNotFound(session_id.to_string()))?;

        if !apply(&mut session)? {
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

    /// Attach a kiln to the session's kiln set. Idempotent.
    pub async fn connect_kiln(
        &self,
        session_id: &str,
        kiln: &Path,
        event_tx: Option<&broadcast::Sender<SessionEventMessage>>,
    ) -> Result<Session, AgentError> {
        let kiln = kiln.to_path_buf();
        self.mutate_scope(session_id, event_tx, move |session| {
            Ok(session.add_kiln(kiln))
        })
        .await
    }

    /// Detach a kiln. Any kiln may go, including the one the session was
    /// created with — the set is flat and the session is not stored in it.
    pub async fn disconnect_kiln(
        &self,
        session_id: &str,
        kiln: &Path,
        event_tx: Option<&broadcast::Sender<SessionEventMessage>>,
    ) -> Result<Session, AgentError> {
        self.mutate_scope(session_id, event_tx, |session| {
            let before = session.kilns.len();
            session.kilns.retain(|k| k != kiln);
            Ok(session.kilns.len() != before)
        })
        .await
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
        self.mutate_scope(session_id, event_tx, move |session| {
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
            let new_workspace = workspace.unwrap_or_else(|| {
                session
                    .default_kiln()
                    .map(Path::to_path_buf)
                    .unwrap_or_default()
            });
            if session.workspace == new_workspace {
                return Ok(false);
            }
            session.workspace = new_workspace;
            Ok(true)
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    // Scope-MUTATION behavior is covered end-to-end in
    // tests/rpc_session_scope_e2e.rs — the mutations need a real
    // SessionManager + storage, which the RPC test server provides. What is
    // unit-testable here is the containment set the mutations feed.
    use super::session_containment_roots;
    use crucible_core::session::SessionType;
    use crucible_core::Session;
    use std::path::{Path, PathBuf};

    /// A tools-only agent is a legitimate shape, and it must come out of this
    /// with LESS reach, not unlimited reach. Its own storage dir is the whole
    /// allowed set — no `""` from the absent kiln, no `""` from the absent
    /// workspace, and nothing that would out-rank the sessions-root denial.
    #[test]
    fn a_kilnless_session_gets_only_its_own_storage_directory() {
        let sessions_root = Path::new("/data/sessions");
        let session = Session::new(SessionType::Chat, vec![]);

        let (allowed, denied) = session_containment_roots(&session, sessions_root);

        assert_eq!(allowed, vec![sessions_root.join(&session.id)]);
        assert!(denied.contains(&sessions_root.to_path_buf()));

        // And the same for a set that holds an empty path rather than no path
        // — what a pre-flatten `meta.json` with `"kiln": ""` deserializes to.
        // `""` as an allowed root matches every path at depth 0: containment
        // off, and the sessions-root denial out-ranked by the shallowest
        // possible allow.
        let mut legacy = Session::new(SessionType::Chat, vec![]);
        legacy.kilns.push(PathBuf::new());
        let (allowed, _) = session_containment_roots(&legacy, sessions_root);

        assert_eq!(allowed, vec![sessions_root.join(&legacy.id)]);
    }

    /// Migration is best-effort — kilns that appear mid-run are never scanned
    /// and `migrate_one` skips on failure — so the pre-relocation transcript
    /// directory inside each kiln has to be denied, not assumed empty.
    #[test]
    fn every_kilns_legacy_transcript_directory_is_denied() {
        let sessions_root = Path::new("/data/sessions");
        let session = Session::new(
            SessionType::Chat,
            vec![PathBuf::from("/kilns/a"), PathBuf::from("/kilns/b")],
        );

        let (allowed, denied) = session_containment_roots(&session, sessions_root);

        assert!(allowed.contains(&PathBuf::from("/kilns/a")));
        assert!(allowed.contains(&PathBuf::from("/kilns/b")));
        for kiln in ["/kilns/a", "/kilns/b"] {
            assert!(
                denied.contains(&Path::new(kiln).join(".crucible").join("sessions")),
                "{kiln}'s legacy in-kiln transcripts must be denied: {denied:?}"
            );
        }
    }
}
