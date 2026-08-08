use super::super::*;

use super::create::validate_trust_level;
use crate::agent_manager::AgentError;
use crate::project_manager::forbidden_root_reason;
use crate::trust_resolution::{find_workspace_and_resolve_classification, resolve_provider_trust};
use crucible_core::Session;

/// Daemon-side floor for a caller-supplied directory that becomes session
/// scope — a kiln (indexed, then served by the file API through `kiln.list`)
/// or a workspace (registered as a project, and the containment boundary for
/// the agent's file tools).
///
/// The RPC socket has no authentication, so this is enforced here and not left
/// to whichever client happened to make the call: `session.connect_kiln
/// {"kiln_path": "/"}` otherwise granted the whole filesystem as a read scope
/// without `project.register` ever being involved.
///
/// This is only the floor ([`forbidden_root_reason`] — catastrophic for every
/// caller). Narrower policy for untrusted callers belongs at that caller's
/// boundary; the web routes layer their own on top before reaching this.
pub(crate) fn refuse_forbidden_scope(kind: &str, path: &Path) -> Result<(), String> {
    // Decide on the resolved path so a symlink cannot present an innocent name
    // for a forbidden target. A path that does not resolve is checked as given
    // — it fails later on open/registration anyway.
    let canonical = path.canonicalize();
    let resolved = canonical.as_deref().unwrap_or(path);
    match forbidden_root_reason(resolved, dirs::home_dir().as_deref()) {
        Some(why) => Err(format!(
            "Refusing '{}' as a session {kind}: {why}",
            resolved.display()
        )),
        None => Ok(()),
    }
}

fn scope_response(
    req_id: Option<crucible_core::protocol::RequestId>,
    session: &Session,
) -> Response {
    Response::success(
        req_id,
        serde_json::json!({
            "session_id": session.id,
            "kiln": session.kiln,
            "workspace": session.workspace,
            "connected_kilns": session.connected_kilns,
        }),
    )
}

fn scope_error(req_id: Option<crucible_core::protocol::RequestId>, e: AgentError) -> Response {
    match e {
        AgentError::SessionNotFound(_)
        | AgentError::ConcurrentRequest(_)
        | AgentError::InvalidConfig(_)
        | AgentError::NotSupported(_) => Response::error(req_id, INVALID_PARAMS, e.to_string()),
        other => internal_error(req_id, other),
    }
}

/// Attach-side trust gate: the session's provider must satisfy the target's
/// data classification. Detach never needs this — removing scope can't leak.
fn check_attach_trust(
    sm: &Arc<SessionManager>,
    llm_config: &Option<LlmConfig>,
    session_id: &str,
    classification: Option<DataClassification>,
) -> Result<(), String> {
    let Some(classification) = classification else {
        return Ok(());
    };
    let trust = sm
        .get_session(session_id)
        .and_then(|s| s.agent)
        .map(|agent| resolve_provider_trust(&agent, llm_config.as_ref()))
        .unwrap_or(TrustLevel::Cloud);
    validate_trust_level(trust, classification)
}

pub(crate) async fn handle_session_connect_kiln(
    req: Request,
    sm: &Arc<SessionManager>,
    am: &Arc<AgentManager>,
    km: &Arc<KilnManager>,
    llm_config: &Option<LlmConfig>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str).to_string();
    let kiln_path = PathBuf::from(require_param!(req, "kiln_path", as_str));

    if let Err(message) = refuse_forbidden_scope("kiln", &kiln_path) {
        return Response::error(req.id, INVALID_PARAMS, message);
    }

    // Trust must gate before any side effect: resolving the classification only
    // reads the path's config (no open needed), so a rejected attach leaves the
    // kiln untouched — never discoverable in kiln.list nor indexed.
    let classification = find_workspace_and_resolve_classification(&kiln_path);
    if let Err(message) = check_attach_trust(sm, llm_config, &session_id, classification) {
        return Response::error(req.id, INVALID_PARAMS, message);
    }

    // Opening validates the path and makes the kiln discoverable. An invalid
    // path resolves to no classification above, so the trust gate passes and
    // this still surfaces the clear "Cannot open kiln" error.
    if let Err(e) = km.open(&kiln_path).await {
        return Response::error(
            req.id,
            INVALID_PARAMS,
            format!("Cannot open kiln '{}': {}", kiln_path.display(), e),
        );
    }

    match am
        .connect_kiln(&session_id, &kiln_path, Some(event_tx))
        .await
    {
        Ok(session) => scope_response(req.id, &session),
        Err(e) => scope_error(req.id, e),
    }
}

pub(crate) async fn handle_session_disconnect_kiln(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str).to_string();
    let kiln_path = PathBuf::from(require_param!(req, "kiln_path", as_str));

    match am
        .disconnect_kiln(&session_id, &kiln_path, Some(event_tx))
        .await
    {
        Ok(session) => scope_response(req.id, &session),
        Err(e) => scope_error(req.id, e),
    }
}

pub(crate) async fn handle_session_set_workspace(
    req: Request,
    sm: &Arc<SessionManager>,
    am: &Arc<AgentManager>,
    pm: &Arc<ProjectManager>,
    llm_config: &Option<LlmConfig>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str).to_string();
    // Absent/null → detach (workspace falls back to the kiln path).
    let workspace = optional_param!(req, "workspace", as_str).map(PathBuf::from);

    if let Some(ref ws) = workspace {
        if !ws.is_dir() {
            return Response::error(
                req.id,
                INVALID_PARAMS,
                format!("Workspace is not a directory: {}", ws.display()),
            );
        }
        // Refused outright rather than "registered, and warn if that fails":
        // the workspace is the agent's filesystem containment boundary even
        // when registration is skipped, so a forbidden one must not be set.
        if let Err(message) = refuse_forbidden_scope("workspace", ws) {
            return Response::error(req.id, INVALID_PARAMS, message);
        }
        if let Err(e) = pm.register_if_missing(ws) {
            tracing::warn!(path = %ws.display(), error = %e, "Failed to auto-register project");
        }
        // The project's config may classify the session's kiln.
        let kiln = sm.get_session(&session_id).map(|s| s.kiln);
        let classification = kiln
            .as_deref()
            .and_then(|k| crate::trust_resolution::resolve_kiln_classification(ws, k));
        if let Err(message) = check_attach_trust(sm, llm_config, &session_id, classification) {
            return Response::error(req.id, INVALID_PARAMS, message);
        }
    }

    match am
        .set_workspace(&session_id, workspace, Some(event_tx))
        .await
    {
        Ok(session) => scope_response(req.id, &session),
        Err(e) => scope_error(req.id, e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `session.connect_kiln {"kiln_path": "/"}` used to reach `km.open("/")`
    /// with only a trust check in the way. An open kiln is half of the file
    /// API's `resolve_enclosing_root`, so that granted a read scope over the
    /// whole filesystem — every credential included — without
    /// `project.register` ever being called.
    #[test]
    fn a_session_kiln_may_not_be_the_filesystem_root_or_home() {
        assert!(refuse_forbidden_scope("kiln", Path::new("/")).is_err());
        assert!(refuse_forbidden_scope("kiln", Path::new("/etc")).is_err());
        if let Some(home) = dirs::home_dir() {
            assert!(refuse_forbidden_scope("kiln", &home).is_err());
        }
    }

    /// Same door, other handler: `session.set_workspace` (and `session.create`)
    /// hand the workspace straight to `register_if_missing`, whose failure was
    /// only ever a warning — the session kept the scope regardless.
    #[test]
    fn a_session_workspace_may_not_be_the_filesystem_root_or_home() {
        assert!(refuse_forbidden_scope("workspace", Path::new("/")).is_err());
        if let Some(home) = dirs::home_dir() {
            assert!(refuse_forbidden_scope("workspace", &home).is_err());
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_to_a_forbidden_root_is_refused_as_scope() {
        let tmp = tempfile::TempDir::new().unwrap();
        let link = tmp.path().join("innocent-looking");
        std::os::unix::fs::symlink("/", &link).unwrap();

        assert!(refuse_forbidden_scope("kiln", &link).is_err());
    }

    #[test]
    fn an_ordinary_directory_is_allowed_as_scope() {
        let tmp = tempfile::TempDir::new().unwrap();
        let kiln = tmp.path().join("notes");
        std::fs::create_dir(&kiln).unwrap();

        assert_eq!(refuse_forbidden_scope("kiln", &kiln), Ok(()));
        assert_eq!(refuse_forbidden_scope("workspace", tmp.path()), Ok(()));
    }
}
