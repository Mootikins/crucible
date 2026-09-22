//! `cru.proposals` — the proposal store, as a plugin reads it.
//!
//! The reviewers of the reflection and consolidation passes read the recent
//! rejected proposals before they propose, so that they do not propose the
//! same note again. The daemon owns the store; this module reaches it through
//! [`DaemonSessionApi::rejected_proposals`].

use super::DaemonSessionApi;
use crate::error::LuaError;
use crate::host_registry::Ns;
use crate::lua_util::{gate_module_keys, register_module};
use mlua::{Lua, LuaSerdeExt, Value};
use std::sync::Arc;

/// The number of rows `rejected` answers when the caller gives no limit.
pub(crate) const DEFAULT_REJECTED_LIMIT: usize = 20;

/// The one row shape `rejected` answers. The daemon builds it in
/// `session_bridge.rs`.
const REJECTED_ROW: &str =
    "{ id: string, title: string, reason: string?, paths: { string }, created_at: string }";

/// Every function in `cru.proposals`, with its Luau type. The stub path and
/// the daemon-backed path both read this list, and the key-set gate refuses
/// a table whose keys differ from it.
const PROPOSAL_FNS: &[&str] = &["rejected"];

fn rejected_decl() -> String {
    format!("(limit: number?) -> ({{ {REJECTED_ROW} }}?, string?)")
}

/// Register `cru.proposals` with stub functions that answer
/// `(nil, "no daemon connected")`.
pub(crate) fn register_proposals_stub(lua: &Lua) -> Result<(), LuaError> {
    let mut ns = Ns::new(lua, "cru.proposals")?;
    ns.async_func(
        "rejected",
        &rejected_decl(),
        |lua, _limit: Option<usize>| async move {
            let err = lua.create_string("no daemon connected")?;
            Ok((Value::Nil, Value::String(err)))
        },
    )?;
    publish(lua, &ns)
}

/// Register `cru.proposals` over the daemon's proposal store.
pub(crate) fn register_proposals_with_api(
    lua: &Lua,
    api: Arc<dyn DaemonSessionApi>,
) -> Result<(), LuaError> {
    let mut ns = Ns::new(lua, "cru.proposals")?;
    ns.async_func(
        "rejected",
        &rejected_decl(),
        move |lua, limit: Option<usize>| {
            let api = Arc::clone(&api);
            async move {
                let limit = limit.unwrap_or(DEFAULT_REJECTED_LIMIT);
                match api.rejected_proposals(limit).await {
                    Ok(rows) => {
                        let table = lua.create_table()?;
                        for (i, row) in rows.iter().enumerate() {
                            table.set(i + 1, lua.to_value(row)?)?;
                        }
                        Ok((Value::Table(table), Value::Nil))
                    }
                    Err(e) => Ok((Value::Nil, Value::String(lua.create_string(&e)?))),
                }
            }
        },
    )?;
    publish(lua, &ns)
}

fn publish(lua: &Lua, ns: &Ns<'_>) -> Result<(), LuaError> {
    gate_module_keys("proposals", ns.table(), PROPOSAL_FNS)?;
    register_module(lua, "proposals", ns.table().clone())?;
    Ok(())
}
