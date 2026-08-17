use super::super::*;

use super::create::validate_trust_level;
use crate::agent_manager::AgentError;
use crate::kiln_registry::refuse_forbidden_scope;
use crate::session_manager::KilnScope;
use crate::trust_resolution::{find_workspace_and_resolve_classification, resolve_provider_trust};
use crucible_core::config::KilnName;
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
///
/// # "Said nothing" and "said something unresolvable" are different answers
///
/// An empty scope is permissive at one of these handlers — `session.list` reads
/// it as *every open kiln, plus the data home, plus the kiln-less sessions* —
/// so a request that named kilns and got an empty scope out of them would ask
/// to narrow and be **widened**. That is the empty-set-permits-everything shape
/// again, a module away from the containment builders that already paid for it.
///
/// So the distinction is drawn here, at the parse site, and drawn structurally:
///
/// - No `kilns` and no `kiln` key at all → an empty scope. The caller said
///   nothing, and each handler decides what nothing means for it.
/// - `kilns: []` → an empty scope. An explicitly empty set is the kiln-less
///   caller, whose reach genuinely is nothing.
/// - A **non-empty** `kilns`/`kiln` that resolves to no kiln at all → `Err`,
///   which every caller turns into `INVALID_PARAMS`. Never an empty scope.
///
/// A partially resolvable set keeps the members that resolve: each unresolvable
/// name is dropped with a warning, and dropping is safe *because* the non-empty
/// remainder still narrows. Only the all-dropped case can widen, and that is
/// the case this refuses.
pub(crate) fn caller_kiln_scope(
    req: &Request,
    registry: &crate::kiln_registry::KilnRegistry,
) -> Result<KilnScope, String> {
    // Every element is kept, including the ones that are not strings. The
    // obvious spelling — `filter_map(as_str)` — drops a `7` on the floor, and
    // `{"kilns": [7]}` then arrives here as an empty vector: a request that
    // named a kiln, read as one that named none, and therefore *widened*. A
    // non-string is refused by name below like any other unusable element.
    let raw: Vec<String> = if let Some(kilns) = req.params.get("kilns").and_then(|v| v.as_array()) {
        kilns
            .iter()
            .map(|v| match v.as_str() {
                Some(s) => s.to_string(),
                None => v.to_string(),
            })
            .collect()
    } else {
        optional_param!(req, "kiln", as_str)
            .map(str::to_string)
            .into_iter()
            .collect()
    };
    if raw.is_empty() {
        return Ok(KilnScope::default());
    }

    let mut names = Vec::new();
    let mut refused = Vec::new();
    for value in &raw {
        match KilnName::parse(value) {
            Ok(name) if registry.resolve(&name).registered().is_some() => {
                if !names.contains(&name) {
                    names.push(name);
                }
            }
            Ok(_) | Err(_) => refused.push(value.as_str()),
        }
    }

    if names.is_empty() {
        return Err(format!(
            "None of the kilns named in this request exist: {}. Kilns are addressed by the name \
             of their `[kilns]` entry, not by path.",
            refused
                .iter()
                .map(|v| format!("{v:?}"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !refused.is_empty() {
        tracing::warn!(
            refused = ?refused,
            "Kiln names with no registry entry were dropped from the request's scope"
        );
    }
    Ok(KilnScope::new(names))
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

/// Turn the `kiln` parameter's raw text into a name.
///
/// It used to be `kiln_path` — a directory the caller chose — and that is the
/// door the registration floor now stands in front of. A path here is refused
/// with the reason rather than resolving to nothing: a kiln that never went
/// through registration must not be reachable by naming its directory.
fn parse_scope_kiln(raw: &str) -> Result<KilnName, String> {
    KilnName::parse(raw).map_err(|e| {
        format!(
            "Unknown kiln {raw:?}: {}. Kilns are addressed by the name of their `[kilns]` \
             entry, not by path.",
            e.reason
        )
    })
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
    let kiln = require_param!(req, "kiln", as_str).to_string();
    let name = match parse_scope_kiln(&kiln) {
        Ok(name) => name,
        Err(message) => return Response::error(req.id, INVALID_PARAMS, message),
    };
    // The registry is the gate now: a name it does not know resolves to no
    // directory, and attaching it would leave the session holding a kiln that
    // grants nothing — an absence, which is the thing consumers misread as
    // "unconstrained". So it is refused here instead of attached.
    let Some(kiln) = sm.kiln_registry().resolve(&name).registered() else {
        return Response::error(
            req.id,
            INVALID_PARAMS,
            format!(
                "Unknown kiln {:?}: no `[kilns]` entry is registered under it. \
                 Register one with `cru kiln register <name> <path>`.",
                name.as_str()
            ),
        );
    };
    let kiln_path = kiln.path().to_path_buf();

    // Trust must gate before any side effect: resolving the classification only
    // reads the path's config (no open needed), so a rejected attach leaves the
    // kiln untouched — never discoverable in kiln.list nor indexed.
    let classification = find_workspace_and_resolve_classification(&kiln_path);
    if let Err(message) = check_attach_trust(sm, llm_config, &session_id, classification) {
        return Response::error(req.id, INVALID_PARAMS, message);
    }

    // Opening makes the kiln discoverable. A registered directory that will not
    // open still surfaces the clear "Cannot open kiln" error.
    if let Err(e) = km.open(&kiln_path).await {
        return Response::error(
            req.id,
            INVALID_PARAMS,
            format!("Cannot open kiln '{name}': {e}"),
        );
    }

    match am.connect_kiln(&session_id, &name, Some(event_tx)).await {
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
    // Detach does not go through the registry: shrinking scope can never leak,
    // and a name whose entry has since been removed is exactly the one a user
    // most needs to be able to drop.
    let kiln = require_param!(req, "kiln", as_str).to_string();
    let name = match parse_scope_kiln(&kiln) {
        Ok(name) => name,
        Err(message) => return Response::error(req.id, INVALID_PARAMS, message),
    };

    match am.disconnect_kiln(&session_id, &name, Some(event_tx)).await {
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
        let kilns = sm.kiln_paths(
            &sm.get_session(&session_id)
                .map(|s| s.kilns)
                .unwrap_or_default(),
        );
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
