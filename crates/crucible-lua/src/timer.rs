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
pub fn register_timer_module(lua: &Lua) -> Result<()> {
    let timer = lua.create_table()?;

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
        lua.create_async_function(|_lua, (secs, func): (f64, Function)| async move {
            if !secs.is_finite() || secs < 0.0 {
                return Err(mlua::Error::runtime(
                    "timeout duration must be a finite non-negative number",
                ));
            }
            let dur = Duration::from_secs_f64(secs);
            match tokio::time::timeout(dur, func.call_async::<Value>(())).await {
                Ok(Ok(result)) => Ok((true, result)),
                Ok(Err(e)) => Ok((false, Value::String(_lua.create_string(e.to_string())?))),
                Err(_) => Ok((false, Value::String(_lua.create_string("timeout")?))),
            }
        })?,
    )?;

    crate::lua_util::register_in_namespaces(lua, "timer", timer)?;
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
        let result = lua
            .load("cru.timer.sleep(0.05)")
            .exec_async()
            .await;

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

        let result = lua
            .load("cru.timer.sleep(0)")
            .exec_async()
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_sleep_negative_errors() {
        let lua = Lua::new();
        register_timer_module(&lua).unwrap();

        let result = lua
            .load("cru.timer.sleep(-1)")
            .exec_async()
            .await;
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
