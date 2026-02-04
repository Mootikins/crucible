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

use mlua::{Function, Lua, Result, Value};
use std::time::Duration;

/// Register the timer module under `cru.timer` and `crucible.timer`.
///
/// Functions:
/// - `timer.sleep(seconds)` — async sleep, yields the coroutine
/// - `timer.timeout(seconds, fn)` — run fn with a timeout, returns (ok, result_or_err)
/// - `timer.clock()` — monotonic wall-clock time in seconds (f64)
pub fn register_timer_module(lua: &Lua) -> Result<()> {
    let timer = lua.create_table()?;

    // Capture a reference instant for monotonic clock
    let epoch = std::time::Instant::now();

    // timer.clock() — monotonic wall-clock time in seconds (high resolution)
    // Unlike os.clock() which returns CPU time, this returns wall time that
    // advances even when the Lua VM is yielded at async points.
    timer.set(
        "clock",
        lua.create_function(move |_lua, ()| Ok(epoch.elapsed().as_secs_f64()))?,
    )?;

    // timer.sleep(seconds) — async, yields until duration elapses
    timer.set(
        "sleep",
        lua.create_async_function(|_lua, secs: f64| async move {
            if !secs.is_finite() || secs < 0.0 {
                return Err(mlua::Error::runtime(
                    "sleep duration must be a finite non-negative number",
                ));
            }
            tokio::time::sleep(Duration::from_secs_f64(secs)).await;
            Ok(())
        })?,
    )?;

    // timer.timeout(seconds, fn) — run fn with timeout
    // Returns (true, result) on success, (false, "timeout") on timeout, (false, err) on error
    timer.set(
        "timeout",
        lua.create_async_function(|lua, (secs, func): (f64, Function)| async move {
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
        })?,
    )?;

    crate::lua_util::register_in_namespaces(lua, "timer", timer)?;

    // cru.spawn(fn) — spawn an async Lua function as an independent task.
    // The function runs concurrently with the caller (fire-and-forget).
    // This is needed when event handlers (called via pcall) need to perform
    // async operations that require yielding (e.g. subscribe, next_event).
    // Requires the `send` feature (mlua/send) since tokio::spawn needs Send.
    #[cfg(feature = "send")]
    {
        let spawn_fn = lua.create_function(|_lua, func: Function| {
            tokio::spawn(async move {
                if let Err(e) = func.call_async::<()>(()).await {
                    tracing::warn!("Spawned Lua task error: {}", e);
                }
            });
            Ok(())
        })?;
        let globals = lua.globals();
        let cru: mlua::Table = globals.get("cru")?;
        cru.set("spawn", spawn_fn)?;
        if let Ok(crucible) = globals.get::<mlua::Table>("crucible") {
            crucible.set("spawn", cru.get::<mlua::Value>("spawn")?)?;
        }
    }

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

        let crucible_ns: Table = lua.globals().get("crucible").unwrap();
        let timer2: Table = crucible_ns.get("timer").unwrap();
        assert!(timer2.get::<Function>("sleep").is_ok());
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
