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

/// The Luau type of `cru.schedule` itself — the callable table.
///
/// One string for both the `send` path and the stub, because the two must
/// present the same contract; only the outcome of a call differs.
///
/// The interval is in SECONDS on every arm: `Duration::from_secs_f64` reads
/// it, and the bare-number form is the same number.
///
/// The handler is called with NO arguments and its result is discarded
/// (`func.call_async::<()>(())`), so it is declared `() -> ...any` — a plain
/// `-> ()` would make a handler that happens to return something a type
/// error against correct plugin code.
const SCHEDULE_DECL: &str = "(spec: { every: number?, interval: number? } | number, \
                             handler: () -> ...any) -> number";

/// The one member `cru.schedule` carries, named once so the `send` path and
/// the stub cannot spell it differently.
const CANCEL: &str = "cancel";

/// The Luau type of `cru.schedule.cancel`.
///
/// `false` is a real answer — the handle is unknown or already cancelled —
/// not a failure.
const CANCEL_DECL: &str = "(handle: number) -> boolean";

/// Declare `cru.schedule`, the callable table.
///
/// `declare_only` because no Rust closure can carry this one: the call is a
/// `__call` metamethod, and `__call` passes the table itself as the first
/// argument, so the closure takes `Variadic<Value>` and its Rust type
/// describes neither accepted shape. The table half of the type — `cancel` —
/// comes from the VM walk, as `cru.log`'s does.
fn declare_schedule_call(lua: &Lua) -> LuaResult<()> {
    let cru = crate::lua_util::get_or_create_namespace(lua, "cru")?;
    let mut root = crate::host_registry::Ns::over(lua, "cru", cru);
    root.declare_only("schedule", SCHEDULE_DECL)
        .map_err(|e| mlua::Error::external(e.to_string()))
}

#[cfg(feature = "send")]
mod inner {
    use std::collections::HashMap;
    use std::sync::atomic::AtomicU64;
    use std::sync::{Arc, Mutex};

    pub(super) static HANDLE_COUNTER: AtomicU64 = AtomicU64::new(1);

    pub(super) type ScheduleHandle = u64;

    pub(super) const MAX_ACTIVE_SCHEDULES: usize = 256;

    /// One live schedule: who created it, and how to stop it.
    pub(super) type LiveSchedule = (
        crate::plugin_context::LuaSource,
        tokio::sync::oneshot::Sender<()>,
    );

    /// Shared state tracking active scheduled tasks so they can be cancelled.
    ///
    /// Uses `std::sync::Mutex` (not tokio) since the lock is held only for
    /// brief insert/remove operations and must be usable from sync contexts.
    #[derive(Clone, Default)]
    pub(super) struct ScheduleRegistry {
        /// Each live schedule: who created it, and how to stop it.
        ///
        /// The source is recorded so `clear_source` can stop a plugin's timers
        /// when the plugin goes inert. Without it a reload left the previous
        /// generation's task running, calling a body from a dead load.
        pub(super) cancellers: Arc<Mutex<HashMap<ScheduleHandle, LiveSchedule>>>,
    }

    /// The VM's schedule registry, in its app data, so `cancel_source` reaches
    /// it without the host threading a handle through every caller.
    pub(super) struct InstalledSchedules(pub(super) ScheduleRegistry);
}

/// Stop every schedule `source` created. Answers how many it stopped.
///
/// `cru.schedule` stays its own store — it owns a tokio task and must not sit
/// behind the per-tool-call lock the handler registry takes — so this is the
/// one thing `clear_source` needs from it.
///
/// **What the stop guarantees, exactly.** The task reads `cancel_rx` in a
/// `select!` beside the interval tick, and it reaches that `select!` only
/// BETWEEN ticks. So a callback that is already running runs to its end, and
/// the cancel stops the NEXT tick. "The source is cleared" therefore means that
/// no further tick of this source starts.
#[cfg(feature = "send")]
pub fn cancel_source(lua: &Lua, source: &crate::plugin_context::LuaSource) -> usize {
    let Some(installed) = lua.app_data_ref::<inner::InstalledSchedules>() else {
        return 0;
    };
    let Ok(mut cancellers) = installed.0.cancellers.lock() else {
        return 0;
    };
    let doomed: Vec<u64> = cancellers
        .iter()
        .filter(|(_, (created_by, _))| created_by == source)
        .map(|(handle, _)| *handle)
        .collect();
    for handle in &doomed {
        if let Some((_, tx)) = cancellers.remove(handle) {
            let _ = tx.send(());
        }
    }
    doomed.len()
}

/// Without the `send` feature no schedule can be created, so none can be
/// stopped.
#[cfg(not(feature = "send"))]
pub fn cancel_source(_lua: &Lua, _owner: &crate::plugin_context::LuaSource) -> usize {
    0
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
    lua.set_app_data(inner::InstalledSchedules(registry.clone()));

    // The table exists before its members do, so `cancel` can be registered
    // through `Ns` — declared and CHECKED against its closure — while the
    // `__call` half, which no closure can carry, is only declared.
    let schedule_table = lua.create_table()?;
    let mut ns = crate::host_registry::Ns::over(lua, "cru.schedule", schedule_table.clone());

    // cru.schedule.cancel(handle) -> bool
    let reg_cancel = registry.clone();
    ns.func(CANCEL, CANCEL_DECL, move |_lua, handle: i64| {
        let mut cancellers = reg_cancel
            .cancellers
            .lock()
            .map_err(|e| mlua::Error::external(format!("schedule lock poisoned: {e}")))?;
        if let Some((_owner, tx)) = cancellers.remove(&(handle as u64)) {
            let _ = tx.send(());
            Ok(true)
        } else {
            Ok(false)
        }
    })
    .map_err(|e| mlua::Error::external(e.to_string()))?;

    // cru.schedule(spec, handler) -> handle_id
    let reg_schedule = registry.clone();
    let schedule_fn = lua.create_function(move |lua, args: mlua::Variadic<Value>| {
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

        // The source that scheduled this, captured HERE and re-entered around
        // every tick. A detached task carries no source of its own, so the
        // callback used to run as if no plugin were running. That is why
        // `cru.storage` refused a scheduled write — the namespace is read
        // from the source at call time, and consolidation's cursor writes have
        // been failing under `pcall` ever since.
        let source = crate::plugin_context::current_source(lua);

        // Insert the cancel sender before spawning so cancel() works immediately
        reg_schedule
            .cancellers
            .lock()
            .map_err(|e| mlua::Error::external(format!("schedule lock poisoned: {e}")))?
            .insert(handle, (source.clone(), cancel_tx));

        let reg_cleanup = reg_schedule.clone();
        let vm = lua.clone();
        tokio::spawn(async move {
            let dur = Duration::from_secs_f64(interval_secs);
            let mut interval = tokio::time::interval(dur);
            // First tick fires immediately — skip it so the callback
            // runs after the first interval elapses.
            interval.tick().await;

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        let previous =
                            crate::plugin_context::set_source(&vm, source.clone());
                        let result = func.call_async::<()>(()).await;
                        // Restored on both paths: an source left behind
                        // attributes whatever runs next to this plugin.
                        crate::plugin_context::set_source(&vm, previous);
                        if let Err(e) = result {
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

    // Make the table callable: cru.schedule(...) invokes __call,
    // cru.schedule.cancel(...) is a regular method.
    let meta = lua.create_table()?;
    meta.set("__call", schedule_fn)?;
    schedule_table.set_metatable(Some(meta))?;

    crate::lua_util::register_module(lua, "schedule", schedule_table)?;
    declare_schedule_call(lua)?;

    Ok(())
}

/// Stub when `send` feature is disabled — schedule creation errors,
/// but cancel is a harmless no-op returning false.
#[cfg(not(feature = "send"))]
pub fn register_schedule_module(lua: &Lua) -> LuaResult<()> {
    let schedule_table = lua.create_table()?;
    let mut ns = crate::host_registry::Ns::over(lua, "cru.schedule", schedule_table.clone());

    // The same declaration as the `send` path, checked against this closure
    // too: without a scheduler there is never a handle to cancel, so `false`
    // is the only answer, and it is the same answer an unknown handle gets.
    ns.func(CANCEL, CANCEL_DECL, |_lua, _handle: i64| Ok(false))
        .map_err(|e| mlua::Error::external(e.to_string()))?;

    let err_fn = lua.create_function(|_lua, _args: mlua::Variadic<Value>| -> LuaResult<Value> {
        Err(mlua::Error::external(
            "cru.schedule requires the 'send' feature (multi-threaded Lua)",
        ))
    })?;

    let meta = lua.create_table()?;
    meta.set("__call", err_fn)?;
    schedule_table.set_metatable(Some(meta))?;

    crate::lua_util::register_module(lua, "schedule", schedule_table)?;
    declare_schedule_call(lua)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mlua::Lua;
    #[cfg(feature = "send")]
    use std::time::Duration;

    #[test]
    fn schedule_module_registers_on_cru() {
        let lua = Lua::new();
        crate::lua_util::get_or_create_namespace(&lua, "cru").unwrap();
        register_schedule_module(&lua).unwrap();

        let has_cru: bool = lua
            .load(r#"return type(cru.schedule) == "table""#)
            .eval()
            .unwrap();
        assert!(has_cru, "cru.schedule should be a table");

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

    /// Move the virtual clock past one timer deadline, then let the
    /// spawned schedule task run. `advance` wakes the timer; the extra
    /// yields give the woken task its turn before the test continues.
    #[cfg(feature = "send")]
    async fn advance_one_tick() {
        tokio::time::advance(Duration::from_millis(60)).await;
        tokio::task::yield_now().await;
        tokio::task::yield_now().await;
    }

    // The clock is paused so the timer fires when the test says so, not
    // when the host is fast enough. This keeps the test free of any
    // hardware dependency.
    #[cfg(feature = "send")]
    #[tokio::test(start_paused = true)]
    async fn schedule_runs_and_can_be_cancelled() {
        let lua = Lua::new();
        crate::lua_util::get_or_create_namespace(&lua, "cru").unwrap();
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

        // The spawned task must poll once to register its timer.
        tokio::task::yield_now().await;

        advance_one_tick().await;
        let count: i64 = lua.load("return _test_count").eval().unwrap();
        assert_eq!(count, 1, "callback should fire once per interval");

        advance_one_tick().await;
        let count: i64 = lua.load("return _test_count").eval().unwrap();
        assert_eq!(count, 2, "callback should fire again on the next interval");

        // Cancel
        let cancelled: bool = lua
            .load(format!("return cru.schedule.cancel({})", handle))
            .eval()
            .unwrap();
        assert!(cancelled, "cancel should return true");

        // Let the task see the cancel before the next deadline arrives.
        tokio::task::yield_now().await;

        // Record count, advance past several deadlines, confirm no more increments
        let count_at_cancel: i64 = lua.load("return _test_count").eval().unwrap();
        advance_one_tick().await;
        advance_one_tick().await;
        advance_one_tick().await;
        let count_after: i64 = lua.load("return _test_count").eval().unwrap();
        assert_eq!(
            count_at_cancel, count_after,
            "callback should stop after cancel"
        );
    }

    /// A scheduled callback runs under the plugin that scheduled it.
    ///
    /// The task is detached, so it carries no context of its own. Without the
    /// capture the callback ran as if NO plugin were running: `cru.storage`
    /// had no namespace to key on and refused the write, which is why
    /// consolidation's cursor writes failed silently under `pcall`.
    #[cfg(feature = "send")]
    #[tokio::test]
    async fn a_scheduled_callback_runs_under_the_plugin_that_scheduled_it() {
        use std::sync::{Arc, Mutex};

        let lua = Lua::new();
        crate::lua_util::get_or_create_namespace(&lua, "cru").unwrap();
        register_schedule_module(&lua).unwrap();

        // A probe, because the plugin name lives in Rust-side app data and
        // Lua deliberately cannot read it.
        let seen: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let recorder = Arc::clone(&seen);
        let probe = lua
            .create_function(move |lua, ()| {
                *recorder.lock().expect("probe lock") =
                    crate::plugin_context::current_plugin_name(lua);
                Ok(())
            })
            .unwrap();
        lua.globals().set("_probe", probe).unwrap();

        let previous = crate::plugin_context::enter_plugin(&lua, "consolidation", false);
        lua.load(r#"cru.schedule(0.05, function() _probe() end)"#)
            .eval_async::<Value>()
            .await
            .unwrap();
        // The scheduling call has returned; the plugin is no longer current.
        crate::plugin_context::set_source(&lua, previous);

        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        assert_eq!(
            seen.lock().expect("probe lock").as_deref(),
            Some("consolidation"),
            "the tick must run under the scheduling plugin, not under nobody"
        );
    }

    #[cfg(feature = "send")]
    #[tokio::test(start_paused = true)]
    async fn schedule_accepts_table_spec() {
        let lua = Lua::new();
        crate::lua_util::get_or_create_namespace(&lua, "cru").unwrap();
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

        // The spawned task must poll once to register its timer.
        tokio::task::yield_now().await;
        advance_one_tick().await;

        let ran: bool = lua.load("return _table_spec_ran").eval().unwrap();
        assert!(ran, "callback should have fired with table spec");

        // Cleanup
        lua.load(format!("cru.schedule.cancel({})", handle))
            .exec()
            .unwrap();
    }
}
