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
    #[cfg(feature = "send")]
    timer.func("spawn", "(task: () -> ()) -> ()", |_lua, func: Function| {
        tokio::spawn(async move {
            if let Err(e) = func.call_async::<()>(()).await {
                tracing::warn!("Spawned Lua task error: {}", e);
            }
        });
        Ok(())
    })?;

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
