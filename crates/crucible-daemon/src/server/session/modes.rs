use super::super::*;
use crate::require_param;

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
pub(crate) async fn handle_session_list_modes(req: Request, am: &Arc<AgentManager>) -> Response {
    let session_id = require_param!(req, "session_id", as_str);

    if let Err(crate::agent_manager::AgentError::SessionNotFound(id)) =
        am.get_session_with_agent(session_id)
    {
        return session_not_found(req.id, &id);
    }

    let state = am.session_modes(session_id);
    let modes: Vec<crucible_core::types::mode::ModeDescriptor> = state
        .available_modes
        .iter()
        .map(crucible_core::types::mode::ModeDescriptor::from)
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
