--- The gateway service must be fail-closed.
---
--- Every declared service is spawned unconditionally when the plugin loads, so
--- an unconfigured daemon used to dial `wss://gateway.discord.gg` on startup and
--- then burn the whole retry budget against a third party, because the missing
--- token surfaces as a table error that the retry predicate treats as retryable.
--- Case three is the other half: a guard that never dials is just as broken, and
--- it is the likelier way this regresses.

local plugin = require("discord")
local service_fn = plugin.services.gateway.fn

--- Invoke the gateway service with `crucible.config` answering from `cfg`, and
--- report whether it reached `cru.ws.connect`.
---
--- `config.get` reads `crucible.config.get("discord." .. key)` inside a pcall,
--- so the suite stubs that lookup rather than the plugin's own accessor. The
--- test VM has no `crucible.config`, hence the table is created and restored.
--- `os.getenv` is stubbed away too: `config.get_token` falls back to
--- DISCORD_BOT_TOKEN, and a developer who has one exported must not turn the
--- no-token case green.
local function dialed_with(cfg)
    crucible = crucible or {}
    local had_config = crucible.config
    local had_ws = cru.ws
    local had_getenv = os.getenv
    local dialed = false

    crucible.config = { get = function(key) return cfg[key] end }
    os.getenv = function() return nil end
    cru.ws = {
        connect = function()
            dialed = true
            -- A string error is not `retryable` to gateway.lua's predicate, so
            -- cru.retry re-raises immediately instead of sleeping through ten
            -- exponential backoffs.
            error("service_test: refusing to dial Discord")
        end,
    }

    pcall(service_fn)

    cru.ws = had_ws
    os.getenv = had_getenv
    crucible.config = had_config
    return dialed
end

describe("gateway service", function()
    it("does not dial when auto_connect is unset", function()
        assert.equals(false, dialed_with({}))
    end)

    it("does not dial when auto_connect is false", function()
        assert.equals(false, dialed_with({ ["discord.auto_connect"] = false }))
    end)

    it("does not dial when auto_connect is true but no token is configured", function()
        assert.equals(false, dialed_with({ ["discord.auto_connect"] = true }))
    end)

    it("dials when auto_connect is true and a token is configured", function()
        assert.equals(true, dialed_with({
            ["discord.auto_connect"] = true,
            ["discord.bot_token"] = "test-token",
        }))
    end)
end)
