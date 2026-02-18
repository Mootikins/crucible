-- health.lua - Plugin self-diagnostics (Neovim vim.health-inspired)
--
-- Provides cru.health module for plugins to report health status.
-- Plugins write a health.lua returning {check = function() ... end}
--
-- Usage:
--   cru.health.start("my-plugin")
--   cru.health.ok("Database connected")
--   cru.health.warn("Cache miss rate high", {"Consider increasing cache size"})
--   cru.health.error("API key missing", {"Set CRUCIBLE_API_KEY env var"})
--   local results = cru.health.get_results()
--   -- results = {
--   --   name = "my-plugin",
--   --   healthy = false,  -- error makes this false
--   --   checks = [
--   --     {level="ok", msg="Database connected"},
--   --     {level="warn", msg="Cache miss rate high", advice=["..."]},
--   --     {level="error", msg="API key missing", advice=["..."]},
--   --   ]
--   -- }

local health = {}

-- Internal state: current check results
local _state = {
    name = nil,
    checks = {},
    healthy = true,
}

-- Start a new health check section (resets state)
function health.start(name)
    cru.check.string(name, "name")
    _state = {
        name = name,
        checks = {},
        healthy = true,
    }
end

-- Add an OK check (does not affect healthy status)
function health.ok(msg)
    cru.check.string(msg, "msg")
    table.insert(_state.checks, {
        level = "ok",
        msg = msg,
    })
end

-- Add a warning check (does not affect healthy status)
function health.warn(msg, advice)
    cru.check.string(msg, "msg")
    if advice ~= nil then
        cru.check.table(advice, "advice")
    end
    table.insert(_state.checks, {
        level = "warn",
        msg = msg,
        advice = advice,
    })
end

-- Add an error check (sets healthy = false)
function health.error(msg, advice)
    cru.check.string(msg, "msg")
    if advice ~= nil then
        cru.check.table(advice, "advice")
    end
    _state.healthy = false
    table.insert(_state.checks, {
        level = "error",
        msg = msg,
        advice = advice,
    })
end

-- Add an info check (does not affect healthy status)
function health.info(msg)
    cru.check.string(msg, "msg")
    table.insert(_state.checks, {
        level = "info",
        msg = msg,
    })
end

-- Get current results and reset state
function health.get_results()
    local results = {
        name = _state.name,
        healthy = _state.healthy,
        checks = _state.checks,
    }
    -- Reset state for next check
    _state = {
        name = nil,
        checks = {},
        healthy = true,
    }
    return results
end

-- Register as global cru.health
cru.health = health
