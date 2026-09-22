//! `cru.proposals` — the proposal store, as a plugin reads and decides it.
//!
//! The reviewers of the reflection and consolidation passes read the recent
//! rejected proposals before they propose, so that they do not propose the
//! same note again. The `review` plugin lists the proposals of a delegated
//! child and accepts or rejects them. The daemon owns the store; this module
//! reaches it through [`DaemonSessionApi`].

use super::{DaemonSessionApi, ProposalDecision};
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
const PROPOSAL_FNS: &[&str] = &["rejected", "list", "accept", "reject"];

const LIST_DECL: &str = "(opts: { session: string?, all: boolean? }?) -> ({ any }?, string?)";
const ACCEPT_DECL: &str = "(params: { id: string, paths: { string }? }) -> (any, string?)";
const REJECT_DECL: &str =
    "(params: { id: string, reason: string?, paths: { string }? }) -> (any, string?)";

/// The options of `list`. An absent table lists the Inbox of every session.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ListOpts {
    session: Option<String>,
    #[serde(default)]
    all: bool,
}

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
    for (name, decl) in [
        ("list", LIST_DECL),
        ("accept", ACCEPT_DECL),
        ("reject", REJECT_DECL),
    ] {
        ns.async_func(name, decl, |lua, _params: Value| async move {
            let err = lua.create_string("no daemon connected")?;
            Ok((Value::Nil, Value::String(err)))
        })?;
    }
    publish(lua, &ns)
}

/// Register `cru.proposals` over the daemon's proposal store.
pub(crate) fn register_proposals_with_api(
    lua: &Lua,
    api: Arc<dyn DaemonSessionApi>,
) -> Result<(), LuaError> {
    let mut ns = Ns::new(lua, "cru.proposals")?;
    let rejected_api = Arc::clone(&api);
    ns.async_func(
        "rejected",
        &rejected_decl(),
        move |lua, limit: Option<usize>| {
            let api = Arc::clone(&rejected_api);
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
    let list_api = Arc::clone(&api);
    ns.async_func("list", LIST_DECL, move |lua, opts: Value| {
        let api = Arc::clone(&list_api);
        async move {
            let opts: ListOpts = if opts.is_nil() {
                ListOpts::default()
            } else {
                match lua.from_value(opts) {
                    Ok(o) => o,
                    Err(e) => return error_pair(&lua, format!("invalid list options: {e}")),
                }
            };
            match api.list_proposals(opts.session, opts.all).await {
                Ok(rows) => {
                    let table = lua.create_table()?;
                    for (i, row) in rows.iter().enumerate() {
                        table.set(i + 1, super::diff::json_to_lua(&lua, row)?)?;
                    }
                    Ok((Value::Table(table), Value::Nil))
                }
                Err(e) => error_pair(&lua, e),
            }
        }
    })?;
    for (name, decl, decision) in [
        ("accept", ACCEPT_DECL, ProposalDecision::Accept),
        ("reject", REJECT_DECL, ProposalDecision::Reject),
    ] {
        let api = Arc::clone(&api);
        ns.async_func(name, decl, move |lua, params: Value| {
            let api = Arc::clone(&api);
            async move {
                let params: serde_json::Value = match lua.from_value(params) {
                    Ok(v) => v,
                    Err(e) => return error_pair(&lua, format!("invalid {name} params: {e}")),
                };
                match api.decide_proposal(decision, params).await {
                    Ok(reply) => Ok((super::diff::json_to_lua(&lua, &reply)?, Value::Nil)),
                    Err(e) => error_pair(&lua, e),
                }
            }
        })?;
    }
    publish(lua, &ns)
}

fn error_pair(lua: &Lua, message: impl AsRef<str>) -> mlua::Result<(Value, Value)> {
    Ok((
        Value::Nil,
        Value::String(lua.create_string(message.as_ref())?),
    ))
}

fn publish(lua: &Lua, ns: &Ns<'_>) -> Result<(), LuaError> {
    gate_module_keys("proposals", ns.table(), PROPOSAL_FNS)?;
    register_module(lua, "proposals", ns.table().clone())?;
    Ok(())
}
