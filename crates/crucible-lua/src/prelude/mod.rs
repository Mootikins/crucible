//! The prelude — Crucible's own pure-Lua additions.
//!
//! Not the Lua standard library: this directory holds what Crucible ADDS to
//! it. Provides `cru.retry`, `cru.emitter`, and `cru.check` as embedded Lua
//! source loaded at executor init time — pure Lua building on the
//! Rust-backed timer module.
//!
//! The Rust here is `cru.errors`, and the TYPES of everything else.
//! `host_registry::Ns` checks a declaration against a closure's Rust types,
//! and a function written in Lua has none, so [`declare_lua_prelude`] states
//! those by hand. They are the unchecked half of this file: read the Lua
//! source before you trust one.

use crate::host_registry::Ns;
use crate::lifecycle::{PluginErrorEntry, PluginErrorLog};
use mlua::{Lua, Result};
use std::sync::{Arc, Mutex};

mod health;
mod qol;
mod stdlib;
mod test_mocks;
mod test_runner;

use health::LUA_HEALTH;
use qol::LUA_QOL;
use stdlib::LUA_STDLIB;
use test_mocks::LUA_TEST_MOCKS;
use test_runner::LUA_TEST_RUNNER;

/// Register the plugin test harness: `describe`, `it`, `run_tests` and the
/// busted-style `assert` table.
///
/// Only a VM that RUNS plugin tests gets these. They used to load in every
/// VM, which put `describe`/`it`/`run_tests` in front of every plugin and
/// replaced the global `assert` with a callable table — so `type(assert)`
/// read "table" in production, and a script that captured `assert` got a
/// harness object instead of the language's own function.
pub fn register_test_harness(lua: &Lua) -> Result<()> {
    lua.load(LUA_TEST_RUNNER).set_name("test_runner").exec()?;
    lua.load(LUA_TEST_MOCKS).set_name("test_mocks").exec()?;
    Ok(())
}

/// One entry of `cru.errors.recent`.
const ERROR_ENTRY: &str = "{ plugin: string, error: string, context: string, age_secs: number }";

/// Register the prelude (retry, emitter, check, errors, health, qol).
///
/// Must be called after `setup_globals` creates the `cru` table and after
/// `register_timer_module` (since `cru.retry` depends on `cru.timer.sleep`).
pub fn register_prelude(lua: &Lua) -> Result<()> {
    lua.load(LUA_STDLIB).exec()?;

    let mut errors = Ns::new(lua, "cru.errors").map_err(mlua::Error::external)?;

    // The plugin name, the error text and where it happened. Answers with
    // nothing, and never fails: a VM with no error log simply drops the
    // entry, because an error path that raises is worse than one that
    // forgets.
    errors
        .func(
            "_capture",
            "(plugin: string, error: string, context: string) -> ()",
            |lua, (plugin, error, context): (String, String, String)| -> Result<()> {
                let error_log = lua
                    .app_data_ref::<Arc<Mutex<PluginErrorLog>>>()
                    .map(|shared| Arc::clone(&*shared));

                if let Some(shared) = error_log {
                    if let Ok(mut guard) = shared.lock() {
                        guard.push(PluginErrorEntry {
                            plugin,
                            error,
                            context,
                            timestamp: std::time::Instant::now(),
                        });
                    }
                }

                Ok(())
            },
        )
        .map_err(mlua::Error::external)?;

    // The last `limit` entries, newest last, 10 when the caller says nothing.
    // A VM with no error log answers with an empty table rather than nil.
    errors
        .func(
            "recent",
            &format!("(limit: number?) -> {{ {ERROR_ENTRY} }}"),
            |lua, n: Option<usize>| {
                let limit = n.unwrap_or(10);
                let result = lua.create_table()?;
                let error_log = lua
                    .app_data_ref::<Arc<Mutex<PluginErrorLog>>>()
                    .map(|shared| Arc::clone(&*shared));

                if let Some(shared) = error_log {
                    if let Ok(guard) = shared.lock() {
                        let entries = guard.recent(limit);
                        for (idx, entry) in entries.into_iter().enumerate() {
                            let row = lua.create_table()?;
                            row.set("plugin", entry.plugin.as_str())?;
                            row.set("error", entry.error.as_str())?;
                            row.set("context", entry.context.as_str())?;
                            row.set("age_secs", entry.timestamp.elapsed().as_secs_f64())?;
                            result.set(idx + 1, row)?;
                        }
                    }
                }

                Ok(result)
            },
        )
        .map_err(mlua::Error::external)?;
    errors.publish().map_err(mlua::Error::external)?;

    lua.load(LUA_QOL).set_name("qol").exec()?;
    lua.load(LUA_HEALTH).set_name("health").exec()?;

    declare_lua_prelude(lua).map_err(mlua::Error::external)
}

/// Declare the types of the prelude functions that Lua source defines.
///
/// `Ns::func` cannot carry these: there is no Rust closure, so there is no
/// Rust type to check the declaration against. Each one is therefore read off
/// the Lua source above and written by hand, and each is `declare_only` —
/// unchecked by construction. Convert one to Rust and it moves to `func`.
///
/// The tables themselves already exist, so every namespace is opened with
/// `Ns::over` on the live table and none of them is published.
fn declare_lua_prelude(lua: &Lua) -> std::result::Result<(), crate::error::LuaError> {
    let cru = crate::lua_util::get_or_create_namespace(lua, "cru")?;

    // `cru.check.*` (`stdlib.rs`). Every one RAISES on a mismatch and answers
    // with nothing, which no type can state — `-> ()` is the whole return.
    // `opts.optional` makes a nil value pass, so the value itself is `any`.
    let check: mlua::Table = cru.get("check")?;
    let mut check_ns = Ns::over(lua, "cru.check", check);
    for name in ["string", "number", "boolean", "table", "func"] {
        // `number` reads `min` and `max` too; the others ignore them.
        let opts = if name == "number" {
            "{ optional: boolean?, min: number?, max: number? }"
        } else {
            "{ optional: boolean? }"
        };
        check_ns.declare_only(
            name,
            &format!("(value: any, name: string, opts: {opts}?) -> ()"),
        )?;
    }
    // `choices` is compared with `==` and only the FAILURE path concatenates
    // it, so any element type is accepted.
    check_ns.declare_only(
        "one_of",
        "(value: any, choices: { any }, name: string, opts: { optional: boolean? }?) -> ()",
    )?;

    // `cru.emitter` (`stdlib.rs`). Both answer with the same object, and its
    // methods are called with `:`, so each carries an explicit `self`.
    const EMITTER: &str = "{ \
        on: (self: any, event: string, fn: (...any) -> ...any, owner: any?) -> number, \
        once: (self: any, event: string, fn: (...any) -> ...any, owner: any?) -> number, \
        off: (self: any, event: string, id: number) -> (), \
        emit: (self: any, event: string, ...any) -> (), \
        emit_async: (self: any, event: string, ...any) -> (), \
        count: (self: any, event: string) -> number, \
        unregister_owner: (self: any, owner: any) -> (), \
        off_all: (self: any, event: string?) -> () \
    }";
    let emitter: mlua::Table = cru.get("emitter")?;
    let mut emitter_ns = Ns::over(lua, "cru.emitter", emitter);
    emitter_ns.declare_only("new", &format!("() -> {EMITTER}"))?;
    // The process-wide singleton, made on first use.
    emitter_ns.declare_only("global", &format!("() -> {EMITTER}"))?;

    // `cru.health.*` (`health.rs`). `start` resets the state, the four
    // reporting calls append to it, and `get_results` drains it — so
    // `get_results` before any `start` answers with a nil `name`.
    const HEALTH_CHECK: &str = "{ level: string, msg: string, advice: { string }? }";
    let health: mlua::Table = cru.get("health")?;
    let mut health_ns = Ns::over(lua, "cru.health", health);
    health_ns.declare_only("start", "(name: string) -> ()")?;
    health_ns.declare_only("ok", "(msg: string) -> ()")?;
    health_ns.declare_only("info", "(msg: string) -> ()")?;
    // Only `error` clears `healthy`; `warn` records the advice and leaves it.
    health_ns.declare_only("warn", "(msg: string, advice: { string }?) -> ()")?;
    health_ns.declare_only("error", "(msg: string, advice: { string }?) -> ()")?;
    health_ns.declare_only(
        "get_results",
        &format!("() -> {{ name: string?, healthy: boolean, checks: {{ {HEALTH_CHECK} }} }}"),
    )?;

    // `cru.service.*` (`stdlib.rs`). `define` REGISTERS the service and hands
    // back the descriptor the daemon spawns; it does not start anything, and
    // it RAISES through `cru.check` on a malformed spec rather than answering
    // with nil.
    //
    // `healthy` is optional throughout because a service with no `health`
    // function has no answer to give — nil there means "not asked", which
    // `false` would misreport as "asked and failed".
    const SERVICE_STATUS: &str =
        "{ name: string, desc: string, running: boolean, healthy: boolean? }";
    let service: mlua::Table = cru.get("service")?;
    let mut service_ns = Ns::over(lua, "cru.service", service);
    service_ns.declare_only(
        "define",
        "(spec: { \
           name: string, \
           desc: string, \
           start: () -> (), \
           stop: (() -> ())?, \
           health: (() -> boolean)?, \
           restart: { max_retries: number?, base_delay: number?, max_delay: number? }?, \
           config: { [string]: { default: any?, secret: boolean? } }? \
         }) -> { desc: string, fn: () -> () }",
    )?;
    service_ns.declare_only("list", &format!("() -> {{ {SERVICE_STATUS} }}"))?;
    // Nil for a name no `define` ever registered.
    service_ns.declare_only("status", &format!("(name: string) -> {SERVICE_STATUS}?"))?;
    // Answers whether the name was known, NOT whether the stop succeeded: a
    // `stop` function that raises is logged and swallowed, and the service is
    // still marked stopped.
    service_ns.declare_only("stop", "(name: string) -> boolean")?;

    // The four that live directly on `cru` (`stdlib.rs` and `qol.rs`).
    let mut root = Ns::over(lua, "cru", cru.clone());
    // Answers with whatever `fn` answered with, and RAISES the last error
    // when every attempt failed.
    root.declare_only(
        "retry",
        "(fn: () -> any, opts: { max_retries: number?, base_delay: number?, \
         max_delay: number?, jitter: boolean?, retryable: ((err: any) -> boolean)? }?) -> any",
    )?;
    root.declare_only(
        "inspect",
        "(value: any, opts: { max_depth: number?, indent: string? }?) -> string",
    )?;
    // Variadic in both: `tbl_get` walks a key path, `tbl_deep_extend` merges
    // every table after the behavior.
    root.declare_only("tbl_get", "(t: any, ...any) -> any")?;
    root.declare_only("tbl_deep_extend", "(behavior: string, ...table) -> table")?;

    Ok(())
}

#[cfg(test)]
mod tests;
