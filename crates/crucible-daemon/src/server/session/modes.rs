use super::super::*;
use crate::rpc_client::SessionIdRequest;
use crate::rpc_helpers::typed_params;

/// The modes a session can be in, and which one it is in now.
///
/// Deliberately serialized as `ModeDescriptor` rather than the ACP
/// `SessionModeState` that backs it: `SessionModeState` is `#[non_exhaustive]`
/// with a `_meta` field and an `Arc<str>` newtype id, so clients cannot
/// construct one without a `serde_json` workaround. `ModeDescriptor` is a
/// plain struct with spare `icon`/`color` fields, which leaves room for
/// `cru.modes.review = { icon = "…" }` without a wire change.
///
/// A session with no agent configured still has modes — they come from the Lua
/// registry, not the agent — so only `SessionNotFound` is an error here.
///
/// Each descriptor carries the **effective** review policy, not the configured
/// one: `min(mode_policy, agent_capability)`. An external ACP agent runs tools
/// in its own process, so a pre-write gate arrives after the write and cannot
/// block it; reporting `pre_write` for such a session would be a mode chip
/// lying about a safety property. A session with no agent yet degrades the
/// same way — `enforceable_by` treats anything but `"internal"` as
/// post-turn — because promising enforcement we cannot yet vouch for is the
/// failure that matters.
pub(crate) async fn handle_session_list_modes(req: Request, am: &Arc<AgentManager>) -> Response {
    let params = match typed_params::<SessionIdRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let session_id = &params.session_id;

    let agent_type = match am.get_session_with_agent(session_id) {
        Ok((_, agent)) => agent.agent_type,
        Err(crate::agent_manager::AgentError::SessionNotFound(id)) => {
            return session_not_found(req.id, &id);
        }
        Err(_) => String::new(),
    };

    let state = am.session_modes(session_id);
    let modes: Vec<crucible_core::types::mode::ModeDescriptor> = state
        .available_modes
        .iter()
        .map(crucible_core::types::mode::ModeDescriptor::from)
        .map(|d| d.degraded_for(&agent_type))
        .collect();

    Response::success(
        req.id,
        serde_json::json!({
            "session_id": session_id,
            "current_mode_id": state.current_mode_id.0.as_ref(),
            "modes": modes,
        }),
    )
}

/// Which settings this session can change, and which it cannot.
///
/// A settings panel drew a fixed list of controls, which was wrong for every
/// ACP session: the protocol has no temperature and no token cap, so the web
/// rendered a slider for each that changed nothing an agent would ever read.
/// The daemon now refuses those settings outright, and this is how a client
/// learns which they are before offering them.
///
/// `supported` is the session's answer, not the agent type's: a model switch
/// depends on whether that particular agent advertised a selector at the
/// handshake, so two ACP sessions can answer differently.
pub(crate) async fn handle_session_list_knobs(req: Request, am: &Arc<AgentManager>) -> Response {
    let params = match typed_params::<SessionIdRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let session_id = &params.session_id;

    if let Err(crate::agent_manager::AgentError::SessionNotFound(id)) =
        am.get_session_with_agent(session_id)
    {
        return session_not_found(req.id, &id);
    }

    let knobs: Vec<serde_json::Value> = am
        .session_knobs(session_id)
        .into_iter()
        .map(|(knob, supported)| serde_json::json!({ "id": knob.id(), "supported": supported }))
        .collect();

    Response::success(
        req.id,
        serde_json::json!({ "session_id": session_id, "knobs": knobs }),
    )
}
