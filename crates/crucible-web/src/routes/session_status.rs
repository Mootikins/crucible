//! `/api/session/{id}/status` — the status slots plugins published for a
//! session. Split from `session.rs` (file-size ceiling).

use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::{
    extract::{Path, State},
    Json,
};

/// Proxy `session.status` verbatim.
///
/// The daemon answers `{"status": [{key, plugin, text, level}, …]}`, sorted by
/// key. Nothing here reads a key: slots are keyed precisely so the chrome
/// owner renders any plugin's state generically, and a match on a known key
/// would be this crate learning what one particular plugin does. Every future
/// plugin gets the channel for free, so long as this stays a passthrough.
///
/// A session that published nothing — the overwhelmingly common case, and any
/// session the daemon has never seen — comes back as an empty array, not an
/// error.
pub(super) async fn session_status(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, WebError> {
    let status = state.daemon.session_status(&id).await.daemon_err()?;
    Ok(Json(status))
}
