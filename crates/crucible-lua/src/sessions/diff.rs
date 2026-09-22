//! `cru.diff` — the diffsets of the daemon, as a plugin reads and comments
//! on them.
//!
//! Each function takes the params object of one `diff.*` RPC and answers
//! its result. The daemon runs the same handler as for a client, so a
//! plugin sees the same admission and the same refusals. The `review`
//! plugin reads the session record of a delegated child, or the branch of
//! a child worktree, through these functions.

use super::{DaemonSessionApi, DiffOp};
use crate::error::LuaError;
use crate::host_registry::Ns;
use crate::lua_util::{gate_module_keys, register_module};
use mlua::{Lua, LuaSerdeExt, Value};
use std::sync::Arc;
use strum::IntoEnumIterator;

/// The Luau type of a `DiffsetSource`. `kind` selects the variant:
/// `branch` reads `root`, `base` and `head`; `session_record` reads
/// `session`; `proposal` reads `id`.
const SOURCE: &str = "{ kind: string, root: string?, base: string?, head: string?, \
                      session: string?, id: string? }";

/// The declared type of one `cru.diff` function.
fn decl(op: DiffOp) -> String {
    let params = match op {
        DiffOp::Get | DiffOp::Comments => format!("{{ source: {SOURCE} }}"),
        DiffOp::File => {
            format!("{{ source: {SOURCE}, path: string, root: string?, from: string? }}")
        }
        DiffOp::Comment => format!(
            "{{ source: {SOURCE}, path: string, root: string?, from: string?, \
             side: string?, line_start: number, line_end: number?, body: string, \
             author: string? }}"
        ),
        DiffOp::ResolveComment => format!("{{ source: {SOURCE}, comment_id: string }}"),
    };
    format!("(params: {params}) -> (any, string?)")
}

/// The names in `cru.diff`, in the order of [`DiffOp`].
fn names() -> Vec<&'static str> {
    DiffOp::iter().map(DiffOp::name).collect()
}

/// Register `cru.diff` with stub functions that answer
/// `(nil, "no daemon connected")`.
pub(crate) fn register_diff_stub(lua: &Lua) -> Result<(), LuaError> {
    let mut ns = Ns::new(lua, "cru.diff")?;
    for op in DiffOp::iter() {
        ns.async_func(op.name(), &decl(op), |lua, _params: Value| async move {
            let err = lua.create_string("no daemon connected")?;
            Ok((Value::Nil, Value::String(err)))
        })?;
    }
    publish(lua, &ns)
}

/// Register `cru.diff` over the daemon's `diff.*` handlers.
pub(crate) fn register_diff_with_api(
    lua: &Lua,
    api: Arc<dyn DaemonSessionApi>,
) -> Result<(), LuaError> {
    let mut ns = Ns::new(lua, "cru.diff")?;
    for op in DiffOp::iter() {
        let api = Arc::clone(&api);
        ns.async_func(op.name(), &decl(op), move |lua, params: Value| {
            let api = Arc::clone(&api);
            async move {
                let params: serde_json::Value = match lua.from_value(params) {
                    Ok(v) => v,
                    Err(e) => {
                        let err =
                            lua.create_string(format!("invalid {} params: {e}", op.name()))?;
                        return Ok((Value::Nil, Value::String(err)));
                    }
                };
                match api.diff(op, params).await {
                    Ok(reply) => Ok((json_to_lua(&lua, &reply)?, Value::Nil)),
                    Err(e) => Ok((Value::Nil, Value::String(lua.create_string(&e)?))),
                }
            }
        })?;
    }
    publish(lua, &ns)
}

/// Convert a daemon reply to Lua. A JSON null becomes an absent key, not
/// the null userdata: `diff.file` answers a null text for an added or a
/// deleted file, and a plugin tests that text with `== nil`.
pub(crate) fn json_to_lua(lua: &Lua, value: &serde_json::Value) -> mlua::Result<Value> {
    let options = mlua::serde::SerializeOptions::new()
        .serialize_none_to_null(false)
        .serialize_unit_to_null(false);
    lua.to_value_with(value, options)
}

fn publish(lua: &Lua, ns: &Ns<'_>) -> Result<(), LuaError> {
    gate_module_keys("diff", ns.table(), &names())?;
    register_module(lua, "diff", ns.table().clone())?;
    Ok(())
}
