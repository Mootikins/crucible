//! Pure Lua standard library utilities.
//!
//! Provides `cru.retry`, `cru.emitter`, and `cru.check` as embedded Lua source
//! loaded at executor init time. No new Rust code needed — these are pure Lua
//! building on the Rust-backed timer module.

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

/// Register the pure Lua standard library (retry, emitter, check, test_runner, health).
///
/// Must be called after `setup_globals` creates the `cru` table and after
/// `register_timer_module` (since `cru.retry` depends on `cru.timer.sleep`).
pub fn register_lua_stdlib(lua: &Lua) -> Result<()> {
    lua.load(LUA_TEST_RUNNER).set_name("test_runner").exec()?;
    lua.load(LUA_TEST_MOCKS).set_name("test_mocks").exec()?;
    lua.load(LUA_STDLIB).exec()?;

    let cru = lua.globals().get::<mlua::Table>("cru")?;
    let errors_table = lua.create_table()?;

    let capture_fn = lua.create_function(
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
    )?;
    errors_table.set("_capture", capture_fn)?;

    let recent_fn = lua.create_function(|lua, n: Option<usize>| {
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
    })?;
    errors_table.set("recent", recent_fn)?;
    cru.set("errors", errors_table)?;

    lua.load(LUA_QOL).set_name("qol").exec()?;
    lua.load(LUA_HEALTH).set_name("health").exec()?;
    lua.load("_G.inspect = cru.inspect").exec()
}

#[cfg(test)]
mod tests;
