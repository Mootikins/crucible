//! `cru.ui.*` — asking the user something from a plugin.
//!
//! Every function here builds one [`InteractionRequest`] variant, hands it to
//! the daemon, and parks until a client answers. The seven variants are the
//! closed set the TUI and the web both render; this module adds no eighth
//! shape of its own, because a request no client knows how to draw is a
//! plugin hanging until its timeout.
//!
//! ## Why this is not `cru.sessions.ask`
//!
//! An interaction is addressed to a *client*, not to a session — the session
//! id only says which attached client to route to. Putting these under
//! `cru.sessions` would read as "do something to this conversation", which is
//! what `send_message` does and this does not.
//!
//! ## Usage in Lua
//!
//! ```lua
//! local answer = cru.ui.ask(session, {
//!   question = "Which branch?",
//!   choices = { "main", "develop" },
//!   allow_other = true,
//! })
//! if answer.kind == "cancelled" then return end
//! ```
//!
//! Every function returns `(response, nil)` or `(nil, err)`. A response of
//! `{ kind = "cancelled" }` is a *successful* call that nobody answered —
//! no client attached, the user dismissed the modal, or the timeout elapsed.
//! Plugins must handle it; it is the common case on a headless daemon.

use crate::error::LuaError;
use crate::lua_util::register_in_namespaces;
use crate::sessions::DaemonSessionApi;
use mlua::{Lua, LuaSerdeExt, Table, Value};
use std::sync::Arc;

/// Default seconds to wait for an answer.
///
/// Matches the permission prompt's 300 s rather than picking a second number:
/// both are "a human is expected to look at a modal", and two different
/// answers to that question is one more than the tree needs.
const DEFAULT_TIMEOUT_SECS: u64 = 300;

/// The `InteractionRequest` variants, by their serde tag.
///
/// Re-exported from `crucible-core` rather than restated: the list there is
/// kept complete by an exhaustive match that fails to compile when a variant
/// is added, and a second copy here would be a second thing to forget.
pub const INTERACTION_KINDS: &[&str] = crucible_core::interaction::InteractionRequest::KINDS;

/// Build the request JSON for `kind` from a Lua options table.
///
/// The table is passed through as the variant's body with `kind` stamped on
/// it, rather than each field being read and re-emitted here. Restating the
/// field list would make this module a second definition of seven structs
/// that already exist in `crucible-core`, and the failure mode of the two
/// drifting is a plugin setting a field the daemon silently ignores.
fn request_json(kind: &str, opts: Value) -> Result<serde_json::Value, mlua::Error> {
    let mut value: serde_json::Value = match opts {
        Value::Nil => serde_json::Value::Object(serde_json::Map::new()),
        other => serde_json::to_value(&other).map_err(mlua::Error::external)?,
    };
    let obj = value.as_object_mut().ok_or_else(|| {
        mlua::Error::external(LuaError::Runtime(format!(
            "cru.ui.{kind}: options must be a table"
        )))
    })?;
    // A caller's own `kind` is overwritten rather than rejected: the function
    // name already chose the variant, so a mismatched key is a typo with one
    // sensible reading.
    obj.insert(
        "kind".to_string(),
        serde_json::Value::String(kind.to_string()),
    );
    Ok(value)
}

/// Seconds to wait, from an options table's `timeout` key.
fn timeout_from(opts: &Value) -> u64 {
    let Value::Table(t) = opts else {
        return DEFAULT_TIMEOUT_SECS;
    };
    t.get::<Option<u64>>("timeout")
        .ok()
        .flatten()
        .filter(|secs| *secs > 0)
        .unwrap_or(DEFAULT_TIMEOUT_SECS)
}

/// Register `cru.ui` with stub functions that report no daemon.
///
/// Mirrors `register_sessions_module`: the table exists in every VM so a
/// plugin can `pcall` it and fall back, rather than getting "attempt to index
/// a nil value" in the VM where no daemon is wired.
pub fn register_ui_module(lua: &Lua) -> Result<(), LuaError> {
    let ui = lua.create_table()?;
    for kind in INTERACTION_KINDS {
        let f = lua.create_async_function(|lua, _args: (String, Value)| async move {
            let err = lua.create_string("no daemon connected")?;
            Ok((Value::Nil, Value::String(err)))
        })?;
        ui.set(*kind, f)?;
    }
    register_in_namespaces(lua, "ui", ui)?;
    Ok(())
}

/// Replace the `cru.ui` stubs with daemon-backed implementations.
pub fn register_ui_module_with_api(
    lua: &Lua,
    api: Arc<dyn DaemonSessionApi>,
) -> Result<(), LuaError> {
    register_ui_module(lua)?;

    let globals = lua.globals();
    let cru: Table = globals.get("cru")?;
    let ui: Table = cru.get("ui")?;

    for kind in INTERACTION_KINDS {
        let kind = *kind;
        let a = Arc::clone(&api);
        let f = lua.create_async_function(move |lua, (session_id, opts): (String, Value)| {
            let a = Arc::clone(&a);
            async move {
                let timeout = timeout_from(&opts);
                let request = match request_json(kind, opts) {
                    Ok(r) => r,
                    Err(e) => {
                        let err = lua.create_string(e.to_string())?;
                        return Ok((Value::Nil, Value::String(err)));
                    }
                };
                match a.request_interaction(session_id, request, timeout).await {
                    Ok(response) => Ok((lua.to_value(&response)?, Value::Nil)),
                    Err(e) => {
                        let err = lua.create_string(&e)?;
                        Ok((Value::Nil, Value::String(err)))
                    }
                }
            }
        })?;
        ui.set(kind, f)?;
    }

    Ok(())
}
