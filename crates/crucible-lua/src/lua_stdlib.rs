//! Pure Lua standard library utilities.
//!
//! Provides `cru.retry`, `cru.emitter`, and `cru.check` as embedded Lua source
//! loaded at executor init time. No new Rust code needed — these are pure Lua
//! building on the Rust-backed timer module.

use mlua::{Lua, Result};

const LUA_STDLIB: &str = r#"
-- ============================================================================
-- cru.retry — Exponential backoff with jitter
-- ============================================================================

function cru.retry(fn, opts)
    opts = opts or {}
    local max = opts.max_retries or 3
    local base = opts.base_delay or 1.0
    local cap = opts.max_delay or 60.0
    local use_jitter = opts.jitter ~= false
    local is_retryable = opts.retryable or function() return true end

    for attempt = 0, max do
        local ok, result = pcall(fn)
        if ok then return result end
        if attempt == max then error(result) end
        if not is_retryable(result) then error(result) end

        local delay = math.min(base * (2 ^ attempt), cap)
        if use_jitter then
            delay = delay * (0.5 + math.random() * 0.5)
        end

        -- Honor server-specified retry-after
        if type(result) == "table" and result.after then
            delay = math.max(delay, tonumber(result.after) or delay)
        end

        cru.timer.sleep(delay)
    end
end

-- ============================================================================
-- cru.emitter — Minimal event emitter
-- ============================================================================

do
    local Emitter = {}
    Emitter.__index = Emitter

    function Emitter.new()
        return setmetatable({ _listeners = {} }, Emitter)
    end

    function Emitter:on(event, fn)
        if not self._listeners[event] then
            self._listeners[event] = {}
        end
        local list = self._listeners[event]
        local id = #list + 1
        list[id] = fn
        return id
    end

    function Emitter:once(event, fn)
        local id
        id = self:on(event, function(...)
            self:off(event, id)
            fn(...)
        end)
        return id
    end

    function Emitter:off(event, id)
        if self._listeners[event] then
            self._listeners[event][id] = false
        end
    end

    function Emitter:emit(event, ...)
        local listeners = self._listeners[event]
        if not listeners then return end
        for i = 1, #listeners do
            local fn = listeners[i]
            if fn then
                local ok, err = pcall(fn, ...)
                if not ok then
                    cru.log("warn", "emitter handler error on '" .. event .. "': " .. tostring(err))
                end
            end
        end
    end

    function Emitter:off_all(event)
        if event then
            self._listeners[event] = nil
        else
            self._listeners = {}
        end
    end

    cru.emitter = { new = Emitter.new }
end

-- ============================================================================
-- cru.check — Argument validation
-- ============================================================================

do
    local check = {}

    local function fail(name, expected, got)
        error(string.format("%s: expected %s, got %s", name, expected, type(got)), 3)
    end

    function check.string(val, name, opts)
        if opts and opts.optional and val == nil then return end
        if type(val) ~= "string" then fail(name, "string", val) end
    end

    function check.number(val, name, opts)
        if opts and opts.optional and val == nil then return end
        if type(val) ~= "number" then fail(name, "number", val) end
        if opts then
            if opts.min and val < opts.min then
                error(string.format("%s: must be >= %s, got %s", name, opts.min, val), 2)
            end
            if opts.max and val > opts.max then
                error(string.format("%s: must be <= %s, got %s", name, opts.max, val), 2)
            end
        end
    end

    function check.boolean(val, name, opts)
        if opts and opts.optional and val == nil then return end
        if type(val) ~= "boolean" then fail(name, "boolean", val) end
    end

    function check.table(val, name, opts)
        if opts and opts.optional and val == nil then return end
        if type(val) ~= "table" then fail(name, "table", val) end
    end

    function check.one_of(val, choices, name, opts)
        if opts and opts.optional and val == nil then return end
        for _, v in ipairs(choices) do
            if val == v then return end
        end
        error(string.format("%s: must be one of [%s], got %s",
            name, table.concat(choices, ", "), tostring(val)), 2)
    end

    function check.func(val, name, opts)
        if opts and opts.optional and val == nil then return end
        if type(val) ~= "function" then fail(name, "function", val) end
    end

    cru.check = check
end
"#;

/// Register the pure Lua standard library (retry, emitter, check).
///
/// Must be called after `setup_globals` creates the `cru` table and after
/// `register_timer_module` (since `cru.retry` depends on `cru.timer.sleep`).
pub fn register_lua_stdlib(lua: &Lua) -> Result<()> {
    lua.load(LUA_STDLIB).exec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mlua::Table;

    fn setup_lua() -> Lua {
        let lua = Lua::new();
        // Create cru namespace
        lua.load("cru = cru or {}").exec().unwrap();
        // Need a mock cru.log for emitter error handling
        lua.load(r#"
            cru.log = function(level, msg) end
        "#).exec().unwrap();
        // Need cru.timer.sleep for retry (mock it for fast tests)
        lua.load(r#"
            cru.timer = { sleep = function(secs) end }
        "#).exec().unwrap();
        register_lua_stdlib(&lua).unwrap();
        lua
    }

    #[test]
    fn test_emitter_on_emit() {
        let lua = setup_lua();
        let result: i32 = lua
            .load(
                r#"
                local em = cru.emitter.new()
                local got = 0
                em:on("test", function(v) got = v end)
                em:emit("test", 42)
                return got
                "#,
            )
            .eval()
            .unwrap();
        assert_eq!(result, 42);
    }

    #[test]
    fn test_emitter_once() {
        let lua = setup_lua();
        let result: i32 = lua
            .load(
                r#"
                local em = cru.emitter.new()
                local count = 0
                em:once("test", function() count = count + 1 end)
                em:emit("test")
                em:emit("test")
                return count
                "#,
            )
            .eval()
            .unwrap();
        assert_eq!(result, 1);
    }

    #[test]
    fn test_emitter_off() {
        let lua = setup_lua();
        let result: i32 = lua
            .load(
                r#"
                local em = cru.emitter.new()
                local count = 0
                local id = em:on("test", function() count = count + 1 end)
                em:emit("test")
                em:off("test", id)
                em:emit("test")
                return count
                "#,
            )
            .eval()
            .unwrap();
        assert_eq!(result, 1);
    }

    #[test]
    fn test_emitter_error_handling() {
        let lua = setup_lua();
        // Handler errors should not propagate
        let result: i32 = lua
            .load(
                r#"
                local em = cru.emitter.new()
                local got = 0
                em:on("test", function() error("boom") end)
                em:on("test", function() got = 1 end)
                em:emit("test")
                return got
                "#,
            )
            .eval()
            .unwrap();
        assert_eq!(result, 1);
    }

    #[test]
    fn test_retry_succeeds() {
        let lua = setup_lua();
        let result: (String, i32) = lua
            .load(
                r#"
                local attempts = 0
                local result = cru.retry(function()
                    attempts = attempts + 1
                    if attempts < 3 then error({ retryable = true }) end
                    return "ok"
                end, { max_retries = 5, base_delay = 0.001 })
                return result, attempts
                "#,
            )
            .eval()
            .unwrap();
        assert_eq!(result.0, "ok");
        assert_eq!(result.1, 3);
    }

    #[test]
    fn test_retry_exhausted() {
        let lua = setup_lua();
        let result = lua
            .load(
                r#"
                cru.retry(function()
                    error("always fails")
                end, { max_retries = 2, base_delay = 0.001 })
                "#,
            )
            .exec();
        assert!(result.is_err());
    }

    #[test]
    fn test_retry_non_retryable() {
        let lua = setup_lua();
        let result: i32 = lua
            .load(
                r#"
                local attempts = 0
                pcall(cru.retry, function()
                    attempts = attempts + 1
                    error("fatal")
                end, {
                    max_retries = 5,
                    base_delay = 0.001,
                    retryable = function() return false end,
                })
                return attempts
                "#,
            )
            .eval()
            .unwrap();
        assert_eq!(result, 1);
    }

    #[test]
    fn test_check_string() {
        let lua = setup_lua();
        // Valid
        lua.load(r#"cru.check.string("hello", "name")"#).exec().unwrap();
        // Invalid
        assert!(lua.load(r#"cru.check.string(42, "name")"#).exec().is_err());
        // Optional nil
        lua.load(r#"cru.check.string(nil, "name", { optional = true })"#).exec().unwrap();
        // Optional non-nil wrong type
        assert!(lua.load(r#"cru.check.string(42, "name", { optional = true })"#).exec().is_err());
    }

    #[test]
    fn test_check_number_with_range() {
        let lua = setup_lua();
        lua.load(r#"cru.check.number(5, "count", { min = 1, max = 10 })"#).exec().unwrap();
        assert!(lua.load(r#"cru.check.number(0, "count", { min = 1 })"#).exec().is_err());
        assert!(lua.load(r#"cru.check.number(11, "count", { max = 10 })"#).exec().is_err());
    }

    #[test]
    fn test_check_one_of() {
        let lua = setup_lua();
        lua.load(r#"cru.check.one_of("json", {"json", "text"}, "format")"#).exec().unwrap();
        assert!(lua.load(r#"cru.check.one_of("xml", {"json", "text"}, "format")"#).exec().is_err());
    }

    #[test]
    fn test_check_table() {
        let lua = setup_lua();
        lua.load(r#"cru.check.table({}, "opts")"#).exec().unwrap();
        assert!(lua.load(r#"cru.check.table("string", "opts")"#).exec().is_err());
    }

    #[test]
    fn test_check_modules_exist() {
        let lua = setup_lua();
        let cru: Table = lua.globals().get("cru").unwrap();

        assert!(cru.get::<Table>("emitter").is_ok());
        assert!(cru.get::<Table>("check").is_ok());
        assert!(cru.get::<mlua::Function>("retry").is_ok());
    }

    #[test]
    fn test_emitter_preserves_registration_order() {
        let lua = setup_lua();
        let result: String = lua
            .load(
                r#"
                local em = cru.emitter.new()
                local order = ""
                em:on("test", function() order = order .. "a" end)
                em:on("test", function() order = order .. "b" end)
                em:on("test", function() order = order .. "c" end)
                em:emit("test")
                return order
                "#,
            )
            .eval()
            .unwrap();
        assert_eq!(result, "abc");
    }

    #[tokio::test]
    async fn test_retry_with_real_timer() {
        let lua = Lua::new();
        lua.load("cru = cru or {}").exec().unwrap();
        lua.load(r#"cru.log = function() end"#).exec().unwrap();
        crate::timer::register_timer_module(&lua).unwrap();
        register_lua_stdlib(&lua).unwrap();

        let start = std::time::Instant::now();
        let result: (String, i32) = lua
            .load(
                r#"
                local attempts = 0
                local result = cru.retry(function()
                    attempts = attempts + 1
                    if attempts < 3 then error({ retryable = true }) end
                    return "ok"
                end, { max_retries = 5, base_delay = 0.01 })
                return result, attempts
                "#,
            )
            .eval_async()
            .await
            .unwrap();

        assert_eq!(result.0, "ok");
        assert_eq!(result.1, 3);
        // Verify real async sleep was used (at least some time passed)
        assert!(start.elapsed().as_millis() >= 10);
    }
}
