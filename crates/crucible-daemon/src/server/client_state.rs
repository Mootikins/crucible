//! `client_state.get` / `client_state.set` — one generic, opaque blob store
//! for a client's own state, keyed by `(client, key)`.
//!
//! The web server used to keep this on its own disk: a pane-layout blob and
//! a recently-opened-files list, each read and written straight from a file
//! the web process picked (`crates/crucible-web/src/services/daemon.rs`'s
//! `default_layout_path`/`standalone_layout_path`). That made the location
//! and the durability of a client's own display state a second concern the
//! web process owned, split from every other piece of daemon-held state, and
//! gave a standalone (debug) web instance no isolation from the production
//! one except by picking a different file name.
//!
//! The daemon stores the value opaquely under its own data root and never
//! reads it — the shape belongs entirely to the caller, exactly as the
//! layout blob always has. `client` names the kind of caller (`"web"`,
//! `"web-standalone"`); `key` names one blob within it (`"layout"`,
//! `"recents"`). Both are validated as plain identifiers before they ever
//! reach a path, so a caller cannot use either to escape the store's own
//! directory.

use crate::protocol::{Request, Response};
use crate::rpc_helpers::typed_params;
use crucible_core::protocol::requests::{
    ClientStateGetReply, ClientStateKey, ClientStateSetRequest, StatusReply,
};
use crucible_core::protocol::rpc::INVALID_PARAMS;
use std::path::{Path, PathBuf};

use super::core::{internal_error, typed_success};

/// A `client` or `key` may be at most this long, and may hold only ASCII
/// letters, digits, `-`, `_` and `.` (not `.` or `..` alone) — enough for a
/// caller to name a kind of state without ever writing a path.
fn valid_identifier(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s != "."
        && s != ".."
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

fn state_path(data_home: &Path, client: &str, key: &str) -> Result<PathBuf, &'static str> {
    if !valid_identifier(client) || !valid_identifier(key) {
        return Err("client_state: `client` and `key` must be plain identifiers");
    }
    Ok(data_home
        .join("client_state")
        .join(client)
        .join(format!("{key}.json")))
}

pub(crate) async fn handle_client_state_get(req: Request, data_home: &Path) -> Response {
    let params = match typed_params::<ClientStateKey>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let path = match state_path(data_home, &params.client, &params.key) {
        Ok(path) => path,
        Err(reason) => return Response::error(req.id, INVALID_PARAMS, reason.to_string()),
    };

    let value = match tokio::fs::read(&path).await {
        Ok(bytes) => match serde_json::from_slice(&bytes) {
            Ok(value) => Some(value),
            Err(e) => {
                return internal_error(
                    req.id,
                    format!("stored client state is not valid JSON: {e}"),
                )
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return internal_error(req.id, format!("failed to read client state: {e}")),
    };
    typed_success(req.id, ClientStateGetReply { value })
}

pub(crate) async fn handle_client_state_set(req: Request, data_home: &Path) -> Response {
    let params = match typed_params::<ClientStateSetRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let path = match state_path(data_home, &params.client, &params.key) {
        Ok(path) => path,
        Err(reason) => return Response::error(req.id, INVALID_PARAMS, reason.to_string()),
    };

    let bytes = match serde_json::to_vec(&params.value) {
        Ok(bytes) => bytes,
        Err(e) => return internal_error(req.id, format!("failed to serialize client state: {e}")),
    };
    let write = {
        let path = path.clone();
        tokio::task::spawn_blocking(move || crucible_core::fs::write_private(&path, &bytes)).await
    };
    match write {
        Ok(Ok(())) => typed_success(
            req.id,
            StatusReply {
                status: "ok".to_string(),
            },
        ),
        Ok(Err(e)) => internal_error(req.id, format!("failed to write client state: {e}")),
        Err(e) => internal_error(req.id, format!("client state write task failed: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::RequestId;

    fn request(method: &str, params: serde_json::Value) -> Request {
        Request {
            jsonrpc: "2.0".to_string(),
            id: Some(RequestId::Number(1)),
            method: method.to_string(),
            params,
        }
    }

    #[tokio::test]
    async fn a_value_set_is_the_value_a_later_get_answers() {
        let tmp = tempfile::tempdir().unwrap();
        let set_req = request(
            "client_state.set",
            serde_json::json!({ "client": "web", "key": "layout", "value": {"panes": ["a"]} }),
        );
        let resp = handle_client_state_set(set_req, tmp.path()).await;
        assert!(resp.error.is_none(), "{:?}", resp.error);

        let get_req = request(
            "client_state.get",
            serde_json::json!({ "client": "web", "key": "layout" }),
        );
        let resp = handle_client_state_get(get_req, tmp.path()).await;
        let reply: ClientStateGetReply = serde_json::from_value(resp.result.unwrap()).unwrap();
        assert_eq!(reply.value, Some(serde_json::json!({"panes": ["a"]})));
    }

    #[tokio::test]
    async fn a_key_nothing_ever_set_answers_no_value() {
        let tmp = tempfile::tempdir().unwrap();
        let get_req = request(
            "client_state.get",
            serde_json::json!({ "client": "web", "key": "never-set" }),
        );
        let resp = handle_client_state_get(get_req, tmp.path()).await;
        let reply: ClientStateGetReply = serde_json::from_value(resp.result.unwrap()).unwrap();
        assert_eq!(reply.value, None);
    }

    /// Two different `client` namespaces do not see each other's state — the
    /// isolation a standalone web instance needs from the production one.
    #[tokio::test]
    async fn two_clients_do_not_share_state() {
        let tmp = tempfile::tempdir().unwrap();
        handle_client_state_set(
            request(
                "client_state.set",
                serde_json::json!({ "client": "web", "key": "layout", "value": "production"}),
            ),
            tmp.path(),
        )
        .await;
        handle_client_state_set(
            request(
                "client_state.set",
                serde_json::json!({ "client": "web-standalone", "key": "layout", "value": "debug"}),
            ),
            tmp.path(),
        )
        .await;

        let resp = handle_client_state_get(
            request(
                "client_state.get",
                serde_json::json!({ "client": "web", "key": "layout" }),
            ),
            tmp.path(),
        )
        .await;
        let reply: ClientStateGetReply = serde_json::from_value(resp.result.unwrap()).unwrap();
        assert_eq!(reply.value, Some(serde_json::json!("production")));
    }

    #[tokio::test]
    async fn a_traversal_attempt_in_key_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let resp = handle_client_state_set(
            request(
                "client_state.set",
                serde_json::json!({ "client": "web", "key": "../../etc/passwd", "value": "x"}),
            ),
            tmp.path(),
        )
        .await;
        assert!(resp.error.is_some(), "a traversal key must be refused");
        // Nothing landed under the data home at all: the refusal happens
        // before any file is touched, not after a write escapes.
        assert!(!tmp.path().join("client_state").exists());
    }
}
