use super::super::*;

use super::create::validate_trust_level;
use crate::agent_manager::AgentError;
use crate::project_manager::forbidden_root_reason;
use crate::session_manager::KilnScope;
use crate::tools::path_resolution::ResolvedPath;
use crate::trust_resolution::{find_workspace_and_resolve_classification, resolve_provider_trust};
use crucible_core::Session;

/// The caller's kiln set, as the four backlog-spanning handlers receive it.
///
/// `kilns` is the spelling; `kiln` is the single-member one every pre-flatten
/// caller sends, folded in rather than rejected — the same courtesy `Session`'s
/// deserializer extends to a pre-flatten `meta.json`.
///
/// Parsing lives here, next to the other scope gate, so that
/// `session.search`, `session.list`, `session.list_persisted` and
/// `session.cleanup` cannot drift into accepting different spellings of the
/// same thing. What the scope then *means* is [`KilnScope::overlaps`].
pub(crate) fn caller_kiln_scope(req: &Request) -> KilnScope {
    if let Some(kilns) = req.params.get("kilns").and_then(|v| v.as_array()) {
        return KilnScope::new(
            kilns
                .iter()
                .filter_map(|v| v.as_str())
                .map(PathBuf::from)
                .collect(),
        );
    }
    KilnScope::new(
        optional_param!(req, "kiln", as_str)
            .map(PathBuf::from)
            .into_iter()
            .collect(),
    )
}

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
///
/// `sessions_root` is refused on top of that floor, and it is not a "root that
/// encloses everything" rule — it is the opposite. Tool containment is
/// deepest-match-wins, so an allowed root *inside* the denied sessions root
/// beats the denial: attaching `~/.crucible/sessions/chat-victim` as a kiln (or
/// setting it as a workspace) hands the agent exactly the transcript the deny
/// root exists to close, and the catastrophic-roots floor waves it through
/// because it is neither `/`, home, nor a system tree.
pub(crate) fn refuse_forbidden_scope(
    kind: &str,
    path: &Path,
    sessions_root: &Path,
) -> Result<(), String> {
    // Before resolution, because resolution anchors a relative path at the
    // working directory and `""` would become the daemon's cwd — an ordinary
    // directory that clears the floor. The pre-resolution form refused `""`
    // for an accidental reason (`Path::new("").parent()` is `None`, which
    // `forbidden_root_reason` reads as the filesystem root); it is refused
    // here on purpose, because an empty path names nothing and every builder
    // downstream treats it as a universal root.
    if path.as_os_str().is_empty() {
        return Err(format!(
            "Refusing an empty path as a session {kind}: it names no directory"
        ));
    }

    // Decided on BOTH resolved forms of the path — the `..`-clamped lexical
    // one and the one walked back through its deepest existing ancestor.
    //
    // The lexical form is what closes `{data}/not-yet/../sessions/{victim}`:
    // nothing on that path needs to exist for it to name a transcript, and a
    // resolution that only understands existing directories hands it back with
    // the `..` still in it. The resolved form is what closes a symlink
    // presenting an innocent name for a forbidden target. Neither subsumes the
    // other, and this is a *denial*, so any form landing somewhere forbidden
    // refuses — the broad match is the fail-closed one here.
    //
    // The sessions root gets the same treatment, or a data home behind a
    // symlink (`/tmp` on macOS) would make that rule silently never match.
    let resolved = ResolvedPath::resolve(path);
    for candidate in [resolved.lexical(), resolved.canonical()] {
        if let Some(why) = forbidden_root_reason(candidate, dirs::home_dir().as_deref()) {
            return Err(format!(
                "Refusing '{}' as a session {kind}: {why}",
                candidate.display()
            ));
        }
    }

    // An unset sessions root is skipped rather than resolved: resolution
    // anchors a relative path at the working directory, which would turn `""`
    // into "refuse everything under the daemon's cwd".
    if sessions_root.as_os_str().is_empty() {
        return Ok(());
    }
    let sessions_root = ResolvedPath::resolve(sessions_root);
    for candidate in [resolved.lexical(), resolved.canonical()] {
        for root in [sessions_root.lexical(), sessions_root.canonical()] {
            if candidate.starts_with(root) {
                return Err(format!(
                    "Refusing '{}' as a session {kind}: it is inside the session storage root, \
                     which holds every recorded transcript",
                    candidate.display()
                ));
            }
        }
    }
    Ok(())
}

fn scope_response(
    req_id: Option<crucible_core::protocol::RequestId>,
    session: &Session,
) -> Response {
    Response::success(
        req_id,
        serde_json::json!({
            "session_id": session.id,
            "kilns": session.kilns,
            "workspace": session.workspace,
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

    if let Err(message) = refuse_forbidden_scope("kiln", &kiln_path, sm.sessions_root()) {
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
        if let Err(message) = refuse_forbidden_scope("workspace", ws, sm.sessions_root()) {
            return Response::error(req.id, INVALID_PARAMS, message);
        }
        if let Err(e) = pm.register_if_missing(ws) {
            tracing::warn!(path = %ws.display(), error = %e, "Failed to auto-register project");
        }
        // The project's config may classify the session's kilns; the most
        // restrictive of them is what the new workspace must clear.
        let kilns = sm
            .get_session(&session_id)
            .map(|s| s.kilns)
            .unwrap_or_default();
        let classification =
            crate::trust_resolution::most_restrictive_classification(&kilns, |kiln| {
                crate::trust_resolution::resolve_kiln_classification(ws, kiln)
            });
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
        let sessions_root = Path::new("/nonexistent-sessions-root");
        assert!(refuse_forbidden_scope("kiln", Path::new("/"), sessions_root).is_err());
        assert!(refuse_forbidden_scope("kiln", Path::new("/etc"), sessions_root).is_err());
        if let Some(home) = dirs::home_dir() {
            assert!(refuse_forbidden_scope("kiln", &home, sessions_root).is_err());
        }
    }

    /// Same door, other handler: `session.set_workspace` (and `session.create`)
    /// hand the workspace straight to `register_if_missing`, whose failure was
    /// only ever a warning — the session kept the scope regardless.
    #[test]
    fn a_session_workspace_may_not_be_the_filesystem_root_or_home() {
        let sessions_root = Path::new("/nonexistent-sessions-root");
        assert!(refuse_forbidden_scope("workspace", Path::new("/"), sessions_root).is_err());
        if let Some(home) = dirs::home_dir() {
            assert!(refuse_forbidden_scope("workspace", &home, sessions_root).is_err());
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_to_a_forbidden_root_is_refused_as_scope() {
        let tmp = tempfile::TempDir::new().unwrap();
        let link = tmp.path().join("innocent-looking");
        std::os::unix::fs::symlink("/", &link).unwrap();

        assert!(refuse_forbidden_scope("kiln", &link, &tmp.path().join("sessions")).is_err());
    }

    #[test]
    fn an_ordinary_directory_is_allowed_as_scope() {
        let tmp = tempfile::TempDir::new().unwrap();
        let kiln = tmp.path().join("notes");
        std::fs::create_dir(&kiln).unwrap();
        let sessions_root = tmp.path().join("sessions");

        assert_eq!(
            refuse_forbidden_scope("kiln", &kiln, &sessions_root),
            Ok(())
        );
        assert_eq!(
            refuse_forbidden_scope("workspace", tmp.path(), &sessions_root),
            Ok(())
        );
    }

    /// Containment is deepest-match-wins, so an allowed root *inside* the
    /// denied sessions root beats the denial. Attaching another session's
    /// storage directory as a kiln is therefore a way to re-open the subtree
    /// the denial exists to close — and the catastrophic-roots floor lets it
    /// through, since `~/.crucible/sessions/chat-victim` is neither `/`, home,
    /// nor a system tree. Both doors — `session.create` and
    /// `session.connect_kiln` — go through this one gate.
    #[test]
    fn session_storage_may_not_be_attached_as_scope() {
        let tmp = tempfile::TempDir::new().unwrap();
        let sessions_root = tmp.path().join("sessions");
        let victim = sessions_root.join("chat-victim");
        std::fs::create_dir_all(&victim).unwrap();

        assert!(
            refuse_forbidden_scope("kiln", &victim, &sessions_root).is_err(),
            "another session's storage dir must not be attachable as a kiln"
        );
        assert!(
            refuse_forbidden_scope("kiln", &sessions_root, &sessions_root).is_err(),
            "the sessions root itself must not be attachable as a kiln"
        );
        assert!(
            refuse_forbidden_scope("workspace", &victim, &sessions_root).is_err(),
            "the same door via set_workspace must be shut too"
        );
    }

    /// The rule must survive a symlink, like the rest of the floor: it is
    /// decided on the resolved path.
    #[cfg(unix)]
    #[test]
    fn a_symlink_into_the_sessions_root_is_refused_as_scope() {
        let tmp = tempfile::TempDir::new().unwrap();
        let sessions_root = tmp.path().join("sessions");
        let victim = sessions_root.join("chat-victim");
        std::fs::create_dir_all(&victim).unwrap();
        let link = tmp.path().join("innocent-notes");
        std::os::unix::fs::symlink(&victim, &link).unwrap();

        assert!(refuse_forbidden_scope("kiln", &link, &sessions_root).is_err());
    }

    /// An empty path is not a narrow scope, it is no scope: `Path::starts_with("")`
    /// is true of every path and `"".components()` counts zero, so an empty
    /// root out-ranks every denial at the shallowest possible depth. The
    /// builders drop it; the gate must not be the place it gets blessed.
    #[test]
    fn an_empty_path_is_refused_as_scope() {
        let tmp = tempfile::TempDir::new().unwrap();
        assert!(
            refuse_forbidden_scope("kiln", Path::new(""), &tmp.path().join("sessions")).is_err()
        );
        assert!(
            refuse_forbidden_scope("workspace", Path::new(""), &tmp.path().join("sessions"))
                .is_err()
        );
    }

    /// A data home behind a symlink (`/tmp` on macOS, a relocated
    /// `~/.crucible` anywhere) gives the sessions root two spellings, and the
    /// caller picks which one to name. Judging the caller's spelling against
    /// only the resolved root misses the symlinked one entirely — which is how
    /// the rule ends up silently never matching.
    #[cfg(unix)]
    #[test]
    fn a_kiln_named_through_a_symlinked_data_home_is_refused_as_scope() {
        let tmp = tempfile::TempDir::new().unwrap();
        let real_home = tmp.path().join("real-home");
        let innocent = tmp.path().join("notes");
        std::fs::create_dir_all(real_home.join("sessions")).unwrap();
        std::fs::create_dir_all(&innocent).unwrap();
        std::os::unix::fs::symlink(&real_home, tmp.path().join("home")).unwrap();
        // The decoy resolves out of the sessions root, so only its NAME —
        // spelled through the symlinked data home — places it there.
        let decoy = tmp.path().join("home").join("sessions").join("chat-decoy");
        std::os::unix::fs::symlink(&innocent, &decoy).unwrap();

        let sessions_root = tmp.path().join("home").join("sessions");
        assert!(
            refuse_forbidden_scope("kiln", &decoy, &sessions_root).is_err(),
            "the sessions-root rule must hold under the spelling the caller used"
        );
    }

    /// The other direction of the same rule, and the one only the lexical form
    /// can catch: a path *named* inside the sessions root that resolves
    /// somewhere innocent. Attaching it would put a sessions-root path into
    /// the session's allowed roots — where, being deeper than the deny root,
    /// it out-ranks the denial — and answering at all tells the caller which
    /// session ids exist.
    #[cfg(unix)]
    #[test]
    fn a_symlink_named_inside_the_sessions_root_is_refused_as_scope() {
        let tmp = tempfile::TempDir::new().unwrap();
        let sessions_root = tmp.path().join("sessions");
        let innocent = tmp.path().join("notes");
        std::fs::create_dir_all(&sessions_root).unwrap();
        std::fs::create_dir_all(&innocent).unwrap();
        let decoy = sessions_root.join("chat-decoy");
        std::os::unix::fs::symlink(&innocent, &decoy).unwrap();

        assert!(
            refuse_forbidden_scope("kiln", &decoy, &sessions_root).is_err(),
            "a kiln named inside the sessions root must be refused however it resolves"
        );
    }

    /// `canonicalize_lenient` re-appends the un-resolved remainder, so a `..`
    /// that traverses through a directory which does not exist yet survives
    /// into the comparison — and `starts_with` then misses the sessions root
    /// the path actually lands in.
    #[test]
    fn a_traversal_through_a_missing_directory_is_still_refused_as_scope() {
        let tmp = tempfile::TempDir::new().unwrap();
        let data_home = tmp.path();
        let sessions_root = data_home.join("sessions");
        let victim = sessions_root.join("chat-victim");
        std::fs::create_dir_all(&victim).unwrap();

        let dodge = data_home
            .join("not-yet")
            .join("..")
            .join("sessions")
            .join("chat-victim");
        assert!(
            refuse_forbidden_scope("kiln", &dodge, &sessions_root).is_err(),
            "a `..` through a missing directory dodged the sessions-root refusal: {}",
            dodge.display()
        );
    }
}
