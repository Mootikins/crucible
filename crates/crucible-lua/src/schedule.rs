//! Interval-based scheduled callbacks for Lua plugins.
//!
//! Provides `cru.schedule(spec, handler)` for running Lua functions at regular
//! intervals, managed by tokio timers. Requires the `send` feature since
//! callbacks run on spawned tasks.
//!
//! # Example
//!
//! ```lua
//! -- Run every 60 seconds
//! local handle = cru.schedule({ every = 60 }, function()
//!     print("runs every 60 seconds")
//! end)
//!
//! -- Shorthand: pass interval as a number
//! local h2 = cru.schedule(5, function()
//!     print("every 5 seconds")
//! end)
//!
//! -- Cancel later
//! cru.schedule.cancel(handle)
//! ```

use mlua::{Lua, Result as LuaResult, Value};

#[cfg(feature = "send")]
mod inner {
    use std::collections::HashMap;
    use std::sync::atomic::AtomicU64;
    use std::sync::{Arc, Mutex};

    pub(super) static HANDLE_COUNTER: AtomicU64 = AtomicU64::new(1);

    pub(super) type ScheduleHandle = u64;

    pub(super) const MAX_ACTIVE_SCHEDULES: usize = 256;

    /// Shared state tracking active scheduled tasks so they can be cancelled.
    ///
    /// Uses `std::sync::Mutex` (not tokio) since the lock is held only for
    /// brief insert/remove operations and must be usable from sync contexts.
    #[derive(Clone, Default)]
    pub(super) struct ScheduleRegistry {
        pub(super) cancellers:
            Arc<Mutex<HashMap<ScheduleHandle, tokio::sync::oneshot::Sender<()>>>>,
    }
}

/// Register `cru.schedule(spec, handler)` and `cru.schedule.cancel(handle)`.
///
/// The schedule function is a callable table: calling it creates a new
/// interval timer, and `.cancel(handle)` stops one.
///
/// Only available with the `send` feature — spawned tasks require `Send`.
#[cfg(feature = "send")]
pub fn register_schedule_module(lua: &Lua) -> LuaResult<()> {
    use inner::{ScheduleRegistry, HANDLE_COUNTER};
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    let registry = ScheduleRegistry::default();

    // cru.schedule.cancel(handle) -> bool
    let reg_cancel = registry.clone();
    let cancel_fn = lua.create_function(move |_lua, handle: i64| {
        let mut cancellers = reg_cancel
            .cancellers
            .lock()
            .map_err(|e| mlua::Error::external(format!("schedule lock poisoned: {e}")))?;
        if let Some(tx) = cancellers.remove(&(handle as u64)) {
            let _ = tx.send(());
            Ok(true)
        } else {
            Ok(false)
        }
    })?;

    // cru.schedule(spec, handler) -> handle_id
    let reg_schedule = registry.clone();
    let schedule_fn = lua.create_function(move |_lua, args: mlua::Variadic<Value>| {
        // Parse arguments: skip self (from __call), then spec, then handler.
        // When called via __call metamethod, first arg is the table itself.
        let (spec, handler) = match args.len() {
            // Direct call: schedule(spec, handler)
            2 => (args[0].clone(), args[1].clone()),
            // __call: schedule_table(self, spec, handler)
            3 => (args[1].clone(), args[2].clone()),
            n => {
                return Err(mlua::Error::external(format!(
                    "schedule expects (spec, handler), got {} args",
                    n
                )))
            }
        };

        let interval_secs: f64 = match &spec {
            Value::Table(t) => t
                .get::<f64>("every")
                .or_else(|_| t.get::<f64>("interval"))
                .map_err(|_| {
                    mlua::Error::external(
                        "schedule spec table must have an 'every' or 'interval' field",
                    )
                })?,
            Value::Number(n) => *n,
            Value::Integer(n) => *n as f64,
            _ => {
                return Err(mlua::Error::external(
                    "schedule spec must be a table with 'every' field or a number of seconds",
                ))
            }
        };

        if !interval_secs.is_finite() || interval_secs <= 0.0 {
            return Err(mlua::Error::external(
                "schedule interval must be a finite positive number",
            ));
        }

        let func = match handler {
            Value::Function(f) => f,
            _ => return Err(mlua::Error::external("schedule handler must be a function")),
        };

        let handle = HANDLE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel::<()>();

        // Enforce a cap on active schedules to prevent runaway resource use
        {
            let cancellers = reg_schedule
                .cancellers
                .lock()
                .map_err(|e| mlua::Error::external(format!("schedule lock poisoned: {e}")))?;
            let count = cancellers.len();
            if count >= inner::MAX_ACTIVE_SCHEDULES {
                return Err(mlua::Error::external(format!(
                    "too many active schedules ({}/{})",
                    count,
                    inner::MAX_ACTIVE_SCHEDULES
                )));
            }
        }

        // Insert the cancel sender before spawning so cancel() works immediately
        reg_schedule
            .cancellers
            .lock()
            .map_err(|e| mlua::Error::external(format!("schedule lock poisoned: {e}")))?
            .insert(handle, cancel_tx);

        let reg_cleanup = reg_schedule.clone();
        tokio::spawn(async move {
            let dur = Duration::from_secs_f64(interval_secs);
            let mut interval = tokio::time::interval(dur);
            // First tick fires immediately — skip it so the callback
            // runs after the first interval elapses.
            interval.tick().await;

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        if let Err(e) = func.call_async::<()>(()).await {
                            tracing::warn!(handle, "scheduled callback error: {e}");
                        }
                    }
                    _ = &mut cancel_rx => {
                        break;
                    }
                }
            }

            if let Ok(mut cancellers) = reg_cleanup.cancellers.lock() {
                cancellers.remove(&handle);
            }
        });

        Ok(Value::Integer(handle as i64))
    })?;

    // Build a callable table: cru.schedule(...) invokes __call,
    // cru.schedule.cancel(...) is a regular method.
    let schedule_table = lua.create_table()?;
    schedule_table.set("cancel", cancel_fn)?;

    let meta = lua.create_table()?;
    meta.set("__call", schedule_fn)?;
    schedule_table.set_metatable(Some(meta))?;

    crate::lua_util::register_in_namespaces(lua, "schedule", schedule_table)?;

    Ok(())
}

/// Stub when `send` feature is disabled — schedule creation errors,
/// but cancel is a harmless no-op returning false.
#[cfg(not(feature = "send"))]
pub fn register_schedule_module(lua: &Lua) -> LuaResult<()> {
    let schedule_table = lua.create_table()?;

    let cancel_fn = lua.create_function(|_lua, _handle: i64| Ok(false))?;
    schedule_table.set("cancel", cancel_fn)?;

    let err_fn = lua.create_function(|_lua, _args: mlua::Variadic<Value>| -> LuaResult<Value> {
        Err(mlua::Error::external(
            "cru.schedule requires the 'send' feature (multi-threaded Lua)",
        ))
    })?;

    let meta = lua.create_table()?;
    meta.set("__call", err_fn)?;
    schedule_table.set_metatable(Some(meta))?;

    crate::lua_util::register_in_namespaces(lua, "schedule", schedule_table)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mlua::Lua;

    #[test]
    fn schedule_module_registers_in_namespaces() {
        let lua = Lua::new();
        crate::lua_util::get_or_create_namespace(&lua, "cru").unwrap();
        crate::lua_util::get_or_create_namespace(&lua, "crucible").unwrap();
        register_schedule_module(&lua).unwrap();

        let has_cru: bool = lua
            .load(r#"return type(cru.schedule) == "table""#)
            .eval()
            .unwrap();
        assert!(has_cru, "cru.schedule should be a table");

        let has_crucible: bool = lua
            .load(r#"return type(crucible.schedule) == "table""#)
            .eval()
            .unwrap();
        assert!(has_crucible, "crucible.schedule should be a table");

        let has_cancel: bool = lua
            .load(r#"return type(cru.schedule.cancel) == "function""#)
            .eval()
            .unwrap();
        assert!(has_cancel, "cru.schedule.cancel should be a function");
    }

    #[test]
    fn cancel_nonexistent_handle_returns_false() {
        let lua = Lua::new();
        crate::lua_util::get_or_create_namespace(&lua, "cru").unwrap();
        crate::lua_util::get_or_create_namespace(&lua, "crucible").unwrap();
        register_schedule_module(&lua).unwrap();

        let result: bool = lua
            .load(r#"return cru.schedule.cancel(99999)"#)
            .eval()
            .unwrap();
        assert!(!result);
    }

    #[cfg(feature = "send")]
    #[tokio::test]
    async fn schedule_rejects_non_positive_interval() {
        let lua = Lua::new();
        crate::lua_util::get_or_create_namespace(&lua, "cru").unwrap();
        crate::lua_util::get_or_create_namespace(&lua, "crucible").unwrap();
        register_schedule_module(&lua).unwrap();

        let result = lua
            .load(r#"return cru.schedule(0, function() end)"#)
            .eval_async::<Value>()
            .await;
        assert!(result.is_err(), "zero interval should error");

        let result = lua
            .load(r#"return cru.schedule(-5, function() end)"#)
            .eval_async::<Value>()
            .await;
        assert!(result.is_err(), "negative interval should error");
    }

    #[cfg(feature = "send")]
    #[tokio::test]
    async fn schedule_runs_and_can_be_cancelled() {
        let lua = Lua::new();
        crate::lua_util::get_or_create_namespace(&lua, "cru").unwrap();
        crate::lua_util::get_or_create_namespace(&lua, "crucible").unwrap();
        register_schedule_module(&lua).unwrap();

        // Set up a counter that the callback increments
        lua.load("_test_count = 0").exec().unwrap();

        let handle: i64 = lua
            .load(
                r#"
                return cru.schedule(0.05, function()
                    _test_count = _test_count + 1
                end)
            "#,
            )
            .eval_async()
            .await
            .unwrap();

        assert!(handle > 0, "handle should be positive");

        // Wait for a few ticks
        tokio::time::sleep(std::time::Duration::from_millis(180)).await;

        let count: i64 = lua.load("return _test_count").eval().unwrap();
        assert!(
            count >= 1,
            "callback should have fired at least once, got {count}"
        );

        // Cancel
        let cancelled: bool = lua
            .load(format!("return cru.schedule.cancel({})", handle))
            .eval()
            .unwrap();
        assert!(cancelled, "cancel should return true");

        // Record count, wait, confirm no more increments
        let count_at_cancel: i64 = lua.load("return _test_count").eval().unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(120)).await;
        let count_after: i64 = lua.load("return _test_count").eval().unwrap();
        assert_eq!(
            count_at_cancel, count_after,
            "callback should stop after cancel"
        );
    }

    #[cfg(feature = "send")]
    #[tokio::test]
    async fn schedule_accepts_table_spec() {
        let lua = Lua::new();
        crate::lua_util::get_or_create_namespace(&lua, "cru").unwrap();
        crate::lua_util::get_or_create_namespace(&lua, "crucible").unwrap();
        register_schedule_module(&lua).unwrap();

        lua.load("_table_spec_ran = false").exec().unwrap();

        let handle: i64 = lua
            .load(
                r#"
                return cru.schedule({ every = 0.05 }, function()
                    _table_spec_ran = true
                end)
            "#,
            )
            .eval_async()
            .await
            .unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let ran: bool = lua.load("return _table_spec_ran").eval().unwrap();
        assert!(ran, "callback should have fired with table spec");

        // Cleanup
        lua.load(format!("cru.schedule.cancel({})", handle))
            .exec()
            .unwrap();
    }
}
