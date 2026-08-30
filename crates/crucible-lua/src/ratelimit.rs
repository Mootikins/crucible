//! Rate limiting module for Lua scripts.
//!
//! Provides a token bucket rate limiter as userdata.
//!
//! `interval` is seconds *per token*, not the length of a window, so the
//! sustained rate is `1 / interval` and `capacity` is only the burst allowance.
//!
//! # Example
//!
//! ```lua
//! -- burst of 5, sustained 5 per second
//! local limiter = cru.ratelimit.new({ capacity = 5, interval = 0.2 })
//! limiter:acquire()       -- async: yields until token available
//! limiter:try_acquire()   -- sync: returns true/false
//! limiter:remaining()     -- sync: current token count
//! ```

use mlua::{Lua, Result, Table, UserData, UserDataMethods};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

struct TokenBucket {
    capacity: f64,
    tokens: f64,
    interval: Duration,
    last_refill: Instant,
}

impl TokenBucket {
    fn new(capacity: f64, interval: Duration) -> Self {
        Self {
            capacity,
            tokens: capacity,
            interval,
            last_refill: Instant::now(),
        }
    }

    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill);
        let interval_secs = self.interval.as_secs_f64();
        if interval_secs > 0.0 {
            let new_tokens = elapsed.as_secs_f64() / interval_secs;
            self.tokens = (self.tokens + new_tokens).min(self.capacity);
            self.last_refill = now;
        }
    }

    fn try_acquire(&mut self) -> bool {
        self.refill();
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    fn remaining(&mut self) -> f64 {
        self.refill();
        self.tokens
    }

    fn time_until_token(&mut self) -> Duration {
        self.refill();
        if self.tokens >= 1.0 {
            Duration::ZERO
        } else {
            let needed = 1.0 - self.tokens;
            Duration::from_secs_f64(needed * self.interval.as_secs_f64())
        }
    }
}

/// Rate limiter exposed to Lua as userdata.
struct LuaRateLimiter {
    bucket: Arc<Mutex<TokenBucket>>,
}

impl UserData for LuaRateLimiter {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // acquire() — async, yields until a token is available
        methods.add_async_method("acquire", |_lua, this, ()| async move {
            loop {
                let wait = {
                    let mut bucket = this.bucket.lock().await;
                    if bucket.try_acquire() {
                        return Ok(());
                    }
                    bucket.time_until_token()
                };
                // Sleep outside the lock
                tokio::time::sleep(wait.max(Duration::from_millis(1))).await;
            }
        });

        // try_acquire() — sync, returns true if token was available
        methods.add_method("try_acquire", |_lua, this, ()| {
            // Use try_lock to avoid blocking; if contended, return false
            match this.bucket.try_lock() {
                Ok(mut bucket) => Ok(bucket.try_acquire()),
                Err(_) => Ok(false),
            }
        });

        // remaining() — sync, returns current token count
        methods.add_method("remaining", |_lua, this, ()| match this.bucket.try_lock() {
            Ok(mut bucket) => Ok(bucket.remaining()),
            Err(_) => Ok(0.0),
        });
    }
}

/// What `cru.ratelimit.new` answers with, as Luau can say it.
///
/// The limiter is userdata, which Luau has no way to name, so the methods are
/// declared as a table. Each carries an explicit `self` because a caller
/// writes `limiter:acquire()`.
const LIMITER: &str = "{ \
    acquire: (self: any) -> (), \
    try_acquire: (self: any) -> boolean, \
    remaining: (self: any) -> number \
}";

/// Register the ratelimit module under `cru.ratelimit`.
pub fn register_ratelimit_module(lua: &Lua) -> Result<()> {
    let mut ratelimit =
        crate::host_registry::Ns::new(lua, "cru.ratelimit").map_err(mlua::Error::external)?;

    // Both options have defaults, so `cru.ratelimit.new({})` is a burst of 5
    // at one token per second. A non-positive or non-finite value RAISES
    // rather than silently becoming a limiter that never lets anything
    // through.
    ratelimit
        .func(
            "new",
            &format!("(opts: {{ capacity: number?, interval: number? }}) -> {LIMITER}"),
            |lua, opts: Table| {
                let capacity: f64 = opts.get::<f64>("capacity").unwrap_or(5.0);
                let interval: f64 = opts.get::<f64>("interval").unwrap_or(1.0);

                if !capacity.is_finite() || capacity <= 0.0 {
                    return Err(mlua::Error::runtime(
                        "capacity must be a finite positive number",
                    ));
                }
                if !interval.is_finite() || interval <= 0.0 {
                    return Err(mlua::Error::runtime(
                        "interval must be a finite positive number",
                    ));
                }

                let bucket = TokenBucket::new(capacity, Duration::from_secs_f64(interval));
                let limiter = LuaRateLimiter {
                    bucket: Arc::new(Mutex::new(bucket)),
                };

                // `Value`, not `AnyUserData`: the host cannot name a userdata
                // type in Luau, and the declaration above narrows it.
                Ok(mlua::Value::UserData(lua.create_userdata(limiter)?))
            },
        )
        .map_err(mlua::Error::external)?;

    ratelimit.publish().map_err(mlua::Error::external)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mlua::Function;

    #[tokio::test]
    async fn test_ratelimit_module_registration() {
        let lua = Lua::new();
        register_ratelimit_module(&lua).unwrap();

        let cru: Table = lua.globals().get("cru").unwrap();
        let rl: Table = cru.get("ratelimit").unwrap();
        assert!(rl.get::<Function>("new").is_ok());
    }

    #[tokio::test]
    async fn test_try_acquire_basic() {
        let lua = Lua::new();
        register_ratelimit_module(&lua).unwrap();

        let result = lua
            .load(
                r#"
                local rl = cru.ratelimit.new({ capacity = 2, interval = 10.0 })
                local a = rl:try_acquire()  -- should succeed
                local b = rl:try_acquire()  -- should succeed
                local c = rl:try_acquire()  -- should fail (no tokens)
                return a, b, c
                "#,
            )
            .eval_async::<(bool, bool, bool)>()
            .await;

        assert!(result.is_ok());
        let (a, b, c) = result.unwrap();
        assert!(a, "First acquire should succeed");
        assert!(b, "Second acquire should succeed");
        assert!(!c, "Third acquire should fail");
    }

    #[tokio::test]
    async fn test_remaining() {
        let lua = Lua::new();
        register_ratelimit_module(&lua).unwrap();

        let result = lua
            .load(
                r#"
                local rl = cru.ratelimit.new({ capacity = 3, interval = 10.0 })
                local before = rl:remaining()
                rl:try_acquire()
                local after = rl:remaining()
                return before, after
                "#,
            )
            .eval_async::<(f64, f64)>()
            .await;

        assert!(result.is_ok());
        let (before, after) = result.unwrap();
        assert!((before - 3.0).abs() < 0.1, "Expected ~3, got {before}");
        assert!((after - 2.0).abs() < 0.1, "Expected ~2, got {after}");
    }

    #[tokio::test]
    async fn test_acquire_waits_for_refill() {
        let lua = Lua::new();
        register_ratelimit_module(&lua).unwrap();
        crate::timer::register_timer_module(&lua).unwrap();

        let start = std::time::Instant::now();
        let result = lua
            .load(
                r#"
                local rl = cru.ratelimit.new({ capacity = 1, interval = 0.05 })
                rl:try_acquire()  -- drain the single token
                rl:acquire()      -- should wait ~50ms for refill
                return true
                "#,
            )
            .eval_async::<bool>()
            .await;

        assert!(result.is_ok());
        let elapsed = start.elapsed();
        assert!(
            elapsed >= Duration::from_millis(30),
            "Expected >= 30ms wait, got {:?}",
            elapsed
        );
    }

    #[tokio::test]
    async fn test_invalid_params() {
        let lua = Lua::new();
        register_ratelimit_module(&lua).unwrap();

        let result = lua
            .load(r#"cru.ratelimit.new({ capacity = 0, interval = 1.0 })"#)
            .exec_async()
            .await;
        assert!(result.is_err());

        let result = lua
            .load(r#"cru.ratelimit.new({ capacity = 5, interval = -1.0 })"#)
            .exec_async()
            .await;
        assert!(result.is_err());
    }
}
