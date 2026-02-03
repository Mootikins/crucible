//! Rate limiting module for Lua scripts.
//!
//! Provides a token bucket rate limiter as userdata.
//!
//! # Example
//!
//! ```lua
//! local limiter = cru.ratelimit.new({ capacity = 5, interval = 1.0 })
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
        if elapsed >= self.interval && self.interval.as_secs_f64() > 0.0 {
            let new_tokens = elapsed.as_secs_f64() / self.interval.as_secs_f64();
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
        methods.add_method("remaining", |_lua, this, ()| {
            match this.bucket.try_lock() {
                Ok(mut bucket) => Ok(bucket.remaining()),
                Err(_) => Ok(0.0),
            }
        });
    }
}

/// Register the ratelimit module under `cru.ratelimit` and `crucible.ratelimit`.
pub fn register_ratelimit_module(lua: &Lua) -> Result<()> {
    let ratelimit = lua.create_table()?;

    // ratelimit.new({ capacity = N, interval = secs }) -> LuaRateLimiter
    ratelimit.set(
        "new",
        lua.create_function(|lua, opts: Table| {
            let capacity: f64 = opts.get::<f64>("capacity").unwrap_or(5.0);
            let interval: f64 = opts.get::<f64>("interval").unwrap_or(1.0);

            if capacity <= 0.0 {
                return Err(mlua::Error::runtime("capacity must be positive"));
            }
            if interval <= 0.0 {
                return Err(mlua::Error::runtime("interval must be positive"));
            }

            let bucket = TokenBucket::new(capacity, Duration::from_secs_f64(interval));
            let limiter = LuaRateLimiter {
                bucket: Arc::new(Mutex::new(bucket)),
            };

            lua.create_userdata(limiter)
        })?,
    )?;

    crate::lua_util::register_in_namespaces(lua, "ratelimit", ratelimit)?;
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

        let crucible_ns: Table = lua.globals().get("crucible").unwrap();
        let rl2: Table = crucible_ns.get("ratelimit").unwrap();
        assert!(rl2.get::<Function>("new").is_ok());
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
