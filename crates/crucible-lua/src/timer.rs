//! Timer module for Lua scripts.
//!
//! Provides async sleep and timeout primitives backed by `tokio::time`.
//!
//! # Example
//!
//! ```lua
//! -- Sleep for 2.5 seconds (yields, does not block)
//! cru.timer.sleep(2.5)
//!
//! -- Timeout a function call
//! local ok, result = cru.timer.timeout(5.0, function()
//!     return http.get("https://slow-api.example.com")
//! end)
//! ```

use crate::error::LuaError;
use mlua::{Function, Lua, Value};
use std::time::Duration;

#[cfg(feature = "send")]
mod inner {
    use std::sync::{Arc, Mutex};

    /// One live `cru.timer.spawn` task: who spawned it, and how to stop it.
    pub(super) type LiveTask = (crate::plugin_context::Owner, tokio::task::JoinHandle<()>);

    /// Every task `cru.timer.spawn` started and the runtime has not finished.
    ///
    /// A `Vec`, not a map: `cru.timer.spawn` answers nothing, so no caller can
    /// name one task. "Every task of this owner" is the only question asked of
    /// this store, and [`super::abort_owner`] is the only reader.
    ///
    /// `std::sync::Mutex`, as `cru.schedule` uses, because the lock covers one
    /// push or one drain and must be takeable from a synchronous closure.
    #[derive(Clone, Default)]
    pub(super) struct TaskRegistry {
        pub(super) tasks: Arc<Mutex<Vec<LiveTask>>>,
    }

    /// The VM's task registry, in its app data, so [`super::abort_owner`]
    /// reaches it without the host threading a handle through every caller.
    pub(super) struct InstalledTasks(pub(super) TaskRegistry);
}

/// Abort every task `owner` started through `cru.timer.spawn`. Answers how
/// many handles it aborted.
///
/// **What the abort guarantees, exactly.** `JoinHandle::abort` does not
/// preempt. It marks the task, and the runtime drops the task the next time
/// the task yields:
///
/// - A task parked at an await point — `cru.timer.sleep`, an HTTP call — never
///   resumes. The Lua body stops there, so the lines after the await do not
///   run.
/// - A task inside a stretch of Luau that awaits nothing runs that stretch to
///   its end. Nothing can interrupt it, because the task gives the runtime no
///   point at which to act.
///
/// So "the owner is cleared" means that no further body of this owner STARTS.
/// It does not mean that a body part-way through stops at the call to this
/// function.
#[cfg(feature = "send")]
pub fn abort_owner(lua: &Lua, owner: &crate::plugin_context::Owner) -> usize {
    let Some(installed) = lua.app_data_ref::<inner::InstalledTasks>() else {
        return 0;
    };
    let Ok(mut tasks) = installed.0.tasks.lock() else {
        return 0;
    };
    let mut aborted = 0;
    tasks.retain(|(spawned_by, handle)| {
        if spawned_by == owner {
            handle.abort();
            aborted += 1;
            return false;
        }
        // Another owner's task that the runtime already finished holds a
        // handle nobody can use, so drop it here too.
        !handle.is_finished()
    });
    aborted
}

/// Without the `send` feature no task can be spawned, so none can be aborted.
#[cfg(not(feature = "send"))]
pub fn abort_owner(_lua: &Lua, _owner: &crate::plugin_context::Owner) -> usize {
    0
}

/// Register the timer module under `cru.timer`.
///
/// Every function declares its Luau type beside its closure, and `Ns` holds
/// the declaration to the Rust types at registration. See
/// [`crate::host_registry`].
pub fn register_timer_module(lua: &Lua) -> Result<(), LuaError> {
    let mut timer = crate::host_registry::Ns::new(lua, "cru.timer")?;

    // Capture a reference instant for monotonic clock
    let epoch = std::time::Instant::now();

    // Unlike os.clock() which returns CPU time, this returns wall time that
    // advances even when the Lua VM is yielded at async points.
    timer.func("clock", "() -> number", move |_lua, ()| {
        Ok(epoch.elapsed().as_secs_f64())
    })?;

    // SECONDS, not milliseconds: the closure feeds `Duration::from_secs_f64`.
    // Nothing here can check the NAME, so it is the half a reader must trust —
    // and the half that was wrong before.
    timer.async_func(
        "sleep",
        "(seconds: number) -> ()",
        |_lua, secs: f64| async move {
            if !secs.is_finite() || secs < 0.0 {
                return Err(mlua::Error::runtime(
                    "sleep duration must be a finite non-negative number",
                ));
            }
            tokio::time::sleep(Duration::from_secs_f64(secs)).await;
            Ok(())
        },
    )?;
    timer.doc(
        "sleep",
        "Yield for `seconds`. SECONDS, not milliseconds — the parameter name is \
         the only statement of the unit, and nothing checks a parameter name. \
         Raises on a negative or non-finite duration. Does not block the \
         runtime: other tasks run while this one waits.",
    );

    // Run `body` with a deadline. Answers `(true, result)` on success,
    // `(false, "timeout")` on the deadline, and `(false, message)` when the
    // body raised — so a caller reads the first value to know which.
    timer.async_func(
        "timeout",
        "(seconds: number, body: () -> ...any) -> (boolean, any)",
        |lua, (secs, func): (f64, Function)| async move {
            if !secs.is_finite() || secs < 0.0 {
                return Err(mlua::Error::runtime(
                    "timeout duration must be a finite non-negative number",
                ));
            }
            let dur = Duration::from_secs_f64(secs);
            match tokio::time::timeout(dur, func.call_async::<Value>(())).await {
                Ok(Ok(result)) => Ok((true, result)),
                Ok(Err(e)) => Ok((false, Value::String(lua.create_string(e.to_string())?))),
                Err(_) => Ok((false, Value::String(lua.create_string("timeout")?))),
            }
        },
    )?;
    timer.doc(
        "timeout",
        "Run `body` with a deadline in SECONDS. Answers `(true, result)` when \
         the body finished, `(false, \"timeout\")` when the deadline passed, and \
         `(false, message)` when the body raised — so read the first value to \
         know which of the two the second one is.",
    );

    // timer.spawn(fn) — spawn an async Lua function as an independent task.
    // The function runs concurrently with the caller (fire-and-forget).
    // This is needed when event handlers (called via pcall) need to perform
    // async operations that require yielding (e.g. subscribe, next_event).
    // Requires the `send` feature (mlua/send) since tokio::spawn needs Send.
    //
    // The task is called with no arguments and anything it answers is
    // dropped, so it is declared `() -> ()`.
    //
    // The `JoinHandle` goes into an owner-keyed store in the VM's app data,
    // so `clear_owner` can abort a task when its plugin goes inert. Before
    // this the handle was dropped and the task outlived every generation of
    // the plugin that started it. [`abort_owner`] states what the abort
    // guarantees, which is less than "the task stops now".
    #[cfg(feature = "send")]
    {
        let registry = inner::TaskRegistry::default();
        lua.set_app_data(inner::InstalledTasks(registry.clone()));

        timer.func(
            "spawn",
            "(task: () -> ()) -> ()",
            move |lua, func: Function| {
                // The plugin that spawned this, re-entered around the task,
                // for the reason `cru.schedule` gives: a detached task carries
                // no context of its own, so the task lost its plugin's name —
                // and "no context" is also how the host spells the operator's
                // own authority.
                let owner = crate::plugin_context::current_owner(lua);
                let vm = lua.clone();
                let task_owner = owner.clone();
                let handle = tokio::spawn(async move {
                    let previous = crate::plugin_context::set_owner(&vm, task_owner);
                    let result = func.call_async::<()>(()).await;
                    crate::plugin_context::set_owner(&vm, previous);
                    if let Err(e) = result {
                        tracing::warn!("Spawned Lua task error: {}", e);
                    }
                });
                if let Ok(mut tasks) = registry.tasks.lock() {
                    // Prune here, because nothing else walks the list: a task
                    // that ran to its end leaves a handle nobody can use, and
                    // a plugin that spawns on every event would grow the list
                    // for the life of the daemon.
                    tasks.retain(|(_, live)| !live.is_finished());
                    tasks.push((owner, handle));
                }
                Ok(())
            },
        )?;
    }

    timer.publish()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mlua::{Function, Table};

    #[tokio::test]
    async fn test_timer_module_registration() {
        let lua = Lua::new();
        register_timer_module(&lua).unwrap();

        let cru: Table = lua.globals().get("cru").unwrap();
        let timer: Table = cru.get("timer").unwrap();
        assert!(timer.get::<Function>("sleep").is_ok());
        assert!(timer.get::<Function>("timeout").is_ok());
    }

    /// The root alias is gone: `cru.timer.spawn` is spawn's only address.
    #[cfg(feature = "send")]
    #[tokio::test]
    async fn spawn_lives_only_at_the_timer_address() {
        let lua = Lua::new();
        lua.load("cru = cru or {}").exec().unwrap();
        register_timer_module(&lua).unwrap();

        let root_is_nil: bool = lua.load("return cru.spawn == nil").eval().unwrap();
        assert!(root_is_nil, "cru.spawn is removed");
        let timer_has_it: bool = lua
            .load("return type(cru.timer.spawn) == 'function'")
            .eval()
            .unwrap();
        assert!(timer_has_it, "cru.timer.spawn must answer");
    }

    #[cfg(feature = "send")]
    #[tokio::test]
    async fn timer_spawn_runs_the_function() {
        let lua = Lua::new();
        lua.load("cru = cru or {}").exec().unwrap();
        register_timer_module(&lua).unwrap();

        lua.load(
            r#"
            ran = false
            cru.timer.spawn(function() ran = true end)
            "#,
        )
        .exec_async()
        .await
        .unwrap();

        // The task is independent, so give the runtime a turn to run it.
        for _ in 0..50 {
            if lua.globals().get::<bool>("ran").unwrap() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("the spawned function never ran");
    }

    /// A spawned task runs under the plugin that spawned it.
    ///
    /// Same reason `cru.schedule` carries its owner: the task is detached and
    /// carries no context of its own, so without the capture a plugin's
    /// deferred work lost the name `cru.storage` keys on.
    #[cfg(feature = "send")]
    #[tokio::test]
    async fn a_spawned_task_runs_under_the_plugin_that_spawned_it() {
        use std::sync::{Arc, Mutex};

        let lua = Lua::new();
        lua.load("cru = cru or {}").exec().unwrap();
        register_timer_module(&lua).unwrap();

        // A probe, because the plugin name lives in Rust-side app data and
        // Lua deliberately cannot read it.
        let seen: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let recorder = Arc::clone(&seen);
        let probe = lua
            .create_function(move |lua, ()| {
                *recorder.lock().expect("probe lock") =
                    crate::plugin_context::current_plugin_name(lua);
                Ok(true)
            })
            .unwrap();
        lua.globals().set("_probe", probe).unwrap();
        lua.globals().set("ran", false).unwrap();

        let previous = crate::plugin_context::enter_plugin(&lua, "kanban", false);
        lua.load(r#"cru.timer.spawn(function() ran = _probe() end)"#)
            .exec_async()
            .await
            .unwrap();
        // The spawning call has returned; the plugin is no longer current.
        crate::plugin_context::set_owner(&lua, previous);

        for _ in 0..50 {
            if lua.globals().get::<bool>("ran").unwrap() {
                assert_eq!(
                    seen.lock().expect("probe lock").as_deref(),
                    Some("kanban"),
                    "the task must run under the spawning plugin, not under nobody"
                );
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("the spawned function never ran");
    }

    /// Run the task until it has ticked at least once. A task that never ran
    /// proves nothing about an abort.
    #[cfg(feature = "send")]
    async fn wait_until_ticking(lua: &Lua, global: &str) -> i64 {
        for _ in 0..200 {
            let ticks: i64 = lua.globals().get(global).unwrap();
            if ticks > 0 {
                return ticks;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        panic!("the spawned task never ran, so the abort would prove nothing");
    }

    /// The count after the runtime has had time to act on the abort.
    ///
    /// The abort is not immediate — see [`abort_owner`] — so the count is read
    /// AFTER a settling window and then held still, rather than compared with
    /// the count at the moment of the abort.
    #[cfg(feature = "send")]
    async fn settle(lua: &Lua, global: &str) -> i64 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        lua.globals().get(global).unwrap()
    }

    #[cfg(feature = "send")]
    fn spawn_a_ticking_task(lua: &Lua, plugin: &str, global: &str) {
        lua.globals().set(global, 0).unwrap();
        let previous = crate::plugin_context::enter_plugin(lua, plugin, false);
        lua.load(format!(
            r#"cru.timer.spawn(function()
                 while true do
                   {global} = {global} + 1
                   cru.timer.sleep(0.005)
                 end
               end)"#
        ))
        .exec()
        .unwrap();
        crate::plugin_context::set_owner(lua, previous);
    }

    /// A task a plugin spawned must stop when the plugin's owner is cleared.
    ///
    /// The `JoinHandle` used to be dropped on the floor, so nothing could
    /// abort a spawned task and a plugin marked Not Active left one running
    /// for the life of the daemon.
    #[cfg(feature = "send")]
    #[tokio::test]
    async fn abort_owner_stops_a_task_the_owner_spawned() {
        let lua = Lua::new();
        lua.load("cru = cru or {}").exec().unwrap();
        register_timer_module(&lua).unwrap();

        spawn_a_ticking_task(&lua, "ticker", "ticks");
        wait_until_ticking(&lua, "ticks").await;

        assert_eq!(
            abort_owner(
                &lua,
                &crate::plugin_context::Owner::Plugin("ticker".to_string())
            ),
            1,
            "the store must hold the handle of the task the plugin spawned"
        );

        let stopped_at = settle(&lua, "ticks").await;
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert_eq!(
            lua.globals().get::<i64>("ticks").unwrap(),
            stopped_at,
            "the task kept running after its owner was cleared"
        );
    }

    /// The abort is keyed by owner, so it must not reach another plugin's
    /// task. A clear that stopped every task would make one plugin's reload
    /// break every other plugin.
    #[cfg(feature = "send")]
    #[tokio::test]
    async fn abort_owner_leaves_another_owners_task_running() {
        let lua = Lua::new();
        lua.load("cru = cru or {}").exec().unwrap();
        register_timer_module(&lua).unwrap();

        spawn_a_ticking_task(&lua, "doomed", "doomed_ticks");
        spawn_a_ticking_task(&lua, "spared", "spared_ticks");
        wait_until_ticking(&lua, "doomed_ticks").await;
        wait_until_ticking(&lua, "spared_ticks").await;

        assert_eq!(
            abort_owner(
                &lua,
                &crate::plugin_context::Owner::Plugin("doomed".to_string())
            ),
            1,
            "exactly one task belongs to the cleared owner"
        );

        let doomed_at = settle(&lua, "doomed_ticks").await;
        let spared_at: i64 = lua.globals().get("spared_ticks").unwrap();
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert_eq!(
            lua.globals().get::<i64>("doomed_ticks").unwrap(),
            doomed_at,
            "the cleared owner's task kept running"
        );
        assert!(
            lua.globals().get::<i64>("spared_ticks").unwrap() > spared_at,
            "another owner's task must survive a clear it was not named in"
        );
    }

    /// The call site, not the capability: `clear_owner` is the one door
    /// `make_plugin_inert` uses, so the abort has to hang off it. A store that
    /// works and is never called leaves the invariant as false as before.
    #[cfg(feature = "send")]
    #[tokio::test]
    async fn clear_owner_aborts_the_owners_spawned_task() {
        let lua = Lua::new();
        lua.load("cru = cru or {}").exec().unwrap();
        register_timer_module(&lua).unwrap();

        spawn_a_ticking_task(&lua, "ticker", "ticks");
        wait_until_ticking(&lua, "ticks").await;

        let registry = crate::handlers::LuaScriptHandlerRegistry::new();
        let cleared = crate::handlers::clear_owner(
            &lua,
            &registry,
            &crate::plugin_context::Owner::Plugin("ticker".to_string()),
        );
        assert_eq!(
            cleared, 1,
            "clear_owner must count the task it aborted; it registered nothing else"
        );

        let stopped_at = settle(&lua, "ticks").await;
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert_eq!(
            lua.globals().get::<i64>("ticks").unwrap(),
            stopped_at,
            "clear_owner does not reach the spawned tasks"
        );
    }

    #[tokio::test]
    async fn test_sleep_basic() {
        let lua = Lua::new();
        register_timer_module(&lua).unwrap();

        let start = std::time::Instant::now();
        let result = lua.load("cru.timer.sleep(0.05)").exec_async().await;

        assert!(result.is_ok());
        let elapsed = start.elapsed();
        assert!(
            elapsed >= Duration::from_millis(40),
            "Expected >= 40ms, got {:?}",
            elapsed
        );
    }

    #[tokio::test]
    async fn test_sleep_zero() {
        let lua = Lua::new();
        register_timer_module(&lua).unwrap();

        let result = lua.load("cru.timer.sleep(0)").exec_async().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_sleep_negative_errors() {
        let lua = Lua::new();
        register_timer_module(&lua).unwrap();

        let result = lua.load("cru.timer.sleep(-1)").exec_async().await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("non-negative"), "Got: {err}");
    }

    #[tokio::test]
    async fn test_timeout_success() {
        let lua = Lua::new();
        register_timer_module(&lua).unwrap();

        let result = lua
            .load(
                r#"
                local ok, val = cru.timer.timeout(1.0, function()
                    return 42
                end)
                return ok, val
                "#,
            )
            .eval_async::<(bool, i32)>()
            .await;

        assert!(result.is_ok());
        let (ok, val) = result.unwrap();
        assert!(ok);
        assert_eq!(val, 42);
    }

    #[tokio::test]
    async fn test_timeout_expires() {
        let lua = Lua::new();
        register_timer_module(&lua).unwrap();

        let result = lua
            .load(
                r#"
                local ok, err = cru.timer.timeout(0.05, function()
                    cru.timer.sleep(10)
                    return "should not reach"
                end)
                return ok, err
                "#,
            )
            .eval_async::<(bool, String)>()
            .await;

        assert!(result.is_ok());
        let (ok, err) = result.unwrap();
        assert!(!ok);
        assert_eq!(err, "timeout");
    }
}
