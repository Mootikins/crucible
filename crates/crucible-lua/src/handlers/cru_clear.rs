//! `cru.clear` — the Lua-visible retirement call.
//!
//! The host had this operation twice in Rust and exposed neither to Lua.
//! [`LuaScriptHandlerRegistry::clear_source`] takes all of one source's rows
//! and runs on reload; `clear_session` takes every source's rows for one
//! session and runs at session end. Both belong to the host's own lifecycle,
//! so a plugin author who wanted to retire a handler EARLY had no call at all.
//!
//! `nvim_clear_autocmds` is the most used retirement call in Neovim's shipped
//! runtime — 21 sites, against 10 `nvim_del_autocmd` and 17
//! `nvim_del_augroup` — and its dominant form is `{ group = g, buf = b }`:
//! "my own rows, for this container". This is that call.
//!
//! # The source is implicit, and that is the whole gate
//!
//! A plugin clears its own registrations and cannot reach another plugin's.
//! The source comes from [`current_source`], which reads the VM's Rust-side
//! app data — Lua cannot write it, and a caller therefore cannot spell
//! another source's name. That is the same reason `register` takes the source
//! from the VM rather than from an argument.
//!
//! [`LuaSource`](crate::plugin_context::LuaSource) is TOTAL, so there is no
//! "no source" to refuse: code outside every host bracket answers
//! `LuaSource::UserLua`, which is the user's own `init.lua` and owns whatever
//! it registered. The refusals here are on the `session` option instead,
//! which is where a caller really can name something that is not its own —
//! and they are the same two refusals a scoped registration already makes,
//! read through the same function.

use mlua::{Lua, Result as LuaResult, Table};

use super::hook_name::{every_hook_name, parse_or_suggest};
use super::registry::{session_from_opts, ClearFilter, LuaScriptHandlerRegistry, SessionScope};
use crate::plugin_context::current_source;

/// The name this API reports itself as in every refusal.
const API: &str = "cru.clear";

/// Register `cru.clear`.
///
/// Wired from `register_cru_on_api` rather than from each host that builds a
/// VM. One wiring point, so a VM cannot come up with `cru.on` and without the
/// call that retires what `cru.on` registers — the two write and unwrite one
/// store, and ten call sites each remembering to add the second is the fault
/// this avoids.
pub fn register_cru_clear_api(lua: &Lua, registry: LuaScriptHandlerRegistry) -> LuaResult<()> {
    let clear_fn = lua.create_function(move |lua, opts: Option<Table>| {
        let filter = match &opts {
            Some(opts) => read_filter(lua, opts)?,
            // `cru.clear()` with no table: everything this source registered.
            None => ClearFilter::default(),
        };
        Ok(registry.clear_matching(&current_source(lua), &filter))
    })?;

    crate::lua_util::get_or_create_namespace(lua, "cru")?.set("clear", clear_fn)?;
    crate::host_registry::declare_value(
        lua,
        "cru.clear",
        "(opts: { name: string?, pattern: string?, session: string? }?) -> number",
    )
    .map_err(|e| mlua::Error::external(e.to_string()))?;
    Ok(())
}

/// Read `{ name, pattern, session }` into a filter.
///
/// An unknown `name` RAISES rather than clearing nothing. A typo that
/// silently removes no rows is the same defect `cru.on` closed by refusing a
/// misspelt event: the author reads a success and believes a handler is gone.
fn read_filter(lua: &Lua, opts: &Table) -> LuaResult<ClearFilter> {
    let name = match opts.get::<Option<String>>("name")?.as_deref() {
        Some(spelling) => Some(parse_or_suggest(API, spelling, every_hook_name())?),
        None => None,
    };
    Ok(ClearFilter {
        name,
        pattern: opts.get::<Option<String>>("pattern")?,
        scope: session_from_opts(lua, API, opts)?.map(SessionScope::Session),
    })
}
