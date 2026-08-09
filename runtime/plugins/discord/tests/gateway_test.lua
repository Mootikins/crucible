--- The gateway must actually reconnect.
---
--- `cru.ws.connect` raises a *string* (`mlua::Error::runtime`, ws.rs), never a
--- table, so a predicate that only accepted `err.retryable` rejected every real
--- network failure and `cru.retry` re-raised on the first one: the gateway got
--- zero of its ten attempts. And an unacknowledged heartbeat only logged a
--- warning, so a zombie socket was never dropped and `is_connected` kept
--- claiming a connection that no longer existed. These tests pin the
--- reconnection, not the raise.

local gateway = require("gateway")
local config = require("config")

--- Run `fn` with the clock, the sleep, the socket dialler and the token lookup
--- under the test's control, restoring all four afterwards.
---
--- `config.get_token` is stubbed on the module table rather than through
--- `crucible.config`, because the real one *caches* the first token it resolves
--- and that cache would outlive this file — the suite shares one Lua VM, and
--- `service_test` asserts on the un-cached, no-token path.
local function with_gateway_env(env, fn)
    local had_timer = cru.timer
    local had_ws = cru.ws
    local had_get_token = config.get_token
    local had_random = math.random

    cru.timer = { sleep = function() end, clock = env.clock }
    cru.ws = { connect = env.connect }
    config.get_token = function() return "gateway-test-token" end
    -- Discord's first heartbeat is jittered by `math.random()`; pin it so the
    -- tick the test drives is the tick the loop takes.
    math.random = function() return 0.5 end

    local ok, err = pcall(fn)

    math.random = had_random
    config.get_token = had_get_token
    cru.ws = had_ws
    cru.timer = had_timer
    if not ok then error(err) end
end

local function frame(payload)
    return { type = "text", data = cru.json.encode(payload) }
end

local HELLO = frame({ op = 10, d = { heartbeat_interval = 100 } })
local READY = frame({
    op = 0,
    t = "READY",
    s = 1,
    d = { session_id = "gateway-test", user = { username = "test-bot" } },
})

describe("gateway reconnection", function()
    it("keeps dialling after a connect failure raises a string", function()
        local dials = 0
        local connected_after_ready = false
        local receives = 0

        local socket = {
            send = function() return true end,
            close = function() end,
            receive = function()
                receives = receives + 1
                if receives == 1 then return READY end
                -- READY has landed; end the run the only clean way there is.
                connected_after_ready = gateway.is_connected()
                gateway.disconnect()
                error("gateway_test: stopping the receive loop")
            end,
        }

        with_gateway_env({
            clock = function() return 1000.0 end,
            connect = function()
                dials = dials + 1
                -- Exactly what `cru.ws.connect` raises: a string.
                if dials < 3 then error("gateway_test: simulated network failure") end
                return socket
            end,
        }, function()
            pcall(gateway.connect)
        end)

        assert.equals(3, dials)
        assert.equals(true, connected_after_ready)
    end)

    it("drops a socket whose heartbeat went unacknowledged", function()
        local now = 1000.0
        local dials = 0
        local receives = 0
        local connected_on_redial = nil

        local socket = {
            send = function() return true end,
            close = function() end,
            receive = function()
                receives = receives + 1
                if receives == 1 then return HELLO end
                if receives == 2 then
                    -- Past the 100ms interval, so the next loop pass heartbeats.
                    now = now + 10.0
                    return READY
                end
                if receives == 3 then
                    now = now + 10.0
                    return nil
                end
                error("gateway_test: receive script exhausted")
            end,
        }

        with_gateway_env({
            clock = function() return now end,
            connect = function()
                dials = dials + 1
                if dials == 1 then return socket end
                connected_on_redial = gateway.is_connected()
                gateway.disconnect()
                error("gateway_test: stopping the reconnect")
            end,
        }, function()
            pcall(gateway.connect)
        end)

        -- Three receives, not four: the second unacked heartbeat ends the
        -- connection at the top of the loop, before the socket is read again.
        assert.equals(3, receives)
        assert.equals(2, dials)
        assert.equals(false, connected_on_redial)
    end)
end)

local RESUMED = frame({ op = 0, t = "RESUMED", s = 2, d = {} })
local HEARTBEAT_ACK = frame({ op = 11 })

describe("gateway state across connections", function()
    -- `awaiting_ack` was module state cleared only by an incoming ACK or by
    -- `M.disconnect`'s `if ws then` branch — which the zombie path has already
    -- made unreachable by nilling `ws`. So the flag that ended connection one
    -- was still set when connection two took its first heartbeat, and a
    -- healthy socket dropped itself. Every attempt died the same way, so the
    -- first outage of a daemon's life was permanent.
    it("does not carry a missed ACK from a dropped socket into the next one", function()
        local now = 1000.0
        local dials = 0
        local zombie_receives = 0
        local live_receives = 0

        -- Connection one: HELLO, READY, then silence, so its heartbeat goes
        -- unacknowledged and the zombie check drops it.
        local zombie = {
            send = function() return true end,
            close = function() end,
            receive = function()
                zombie_receives = zombie_receives + 1
                if zombie_receives == 1 then return HELLO end
                now = now + 10.0
                if zombie_receives == 2 then return READY end
                return nil
            end,
        }

        -- Connection two answers every heartbeat. It must survive.
        local live = {
            send = function() return true end,
            close = function() end,
            receive = function()
                live_receives = live_receives + 1
                if live_receives == 1 then return HELLO end
                now = now + 10.0
                if live_receives == 2 then return RESUMED end
                if live_receives <= 5 then return HEARTBEAT_ACK end
                gateway.disconnect()
                error("gateway_test: stopping the receive loop")
            end,
        }

        with_gateway_env({
            clock = function() return now end,
            connect = function()
                dials = dials + 1
                if dials == 1 then return zombie end
                if dials == 2 then return live end
                error("gateway_test: simulated network failure")
            end,
        }, function()
            pcall(gateway.connect)
        end)

        assert.equals(2, dials)
        assert.equals(true, live_receives > 2)
    end)

    -- Only `READY` set `is_connected`, but a socket that resumes gets
    -- `RESUMED` — so a working bot reported itself down. `:discord status`
    -- then told the operator to run `:discord connect`, which passed the
    -- `is_connected()` guard and started a second retry loop over the same
    -- module-local socket.
    it("reports connected after a resume, not only after a ready", function()
        local now = 1000.0
        local dials = 0
        local zombie_receives = 0
        local connected_after_resume = nil

        local zombie = {
            send = function() return true end,
            close = function() end,
            receive = function()
                zombie_receives = zombie_receives + 1
                if zombie_receives == 1 then return HELLO end
                now = now + 10.0
                if zombie_receives == 2 then return READY end
                return nil
            end,
        }

        local resumed = {
            send = function() return true end,
            close = function() end,
            receive = function(self)
                if not self.seen_hello then
                    self.seen_hello = true
                    return HELLO
                end
                if not self.seen_resumed then
                    self.seen_resumed = true
                    return RESUMED
                end
                connected_after_resume = gateway.is_connected()
                gateway.disconnect()
                error("gateway_test: stopping the receive loop")
            end,
        }

        with_gateway_env({
            clock = function() return now end,
            connect = function()
                dials = dials + 1
                if dials == 1 then return zombie end
                return resumed
            end,
        }, function()
            pcall(gateway.connect)
        end)

        assert.equals(true, connected_after_resume)
    end)

    -- `M.disconnect` reset its state inside `if ws then`, but the case that
    -- most needs the reset is an outage, where `ws` is already nil. The flag
    -- stayed true, so `:discord status` claimed a live connection and
    -- `:discord connect` refused with "Already connected" until a restart.
    it("reports disconnected when told to stop during an outage", function()
        local dials = 0

        with_gateway_env({
            clock = function() return 1000.0 end,
            connect = function()
                dials = dials + 1
                if dials == 1 then
                    return {
                        send = function() return true end,
                        close = function() end,
                        receive = function()
                            gateway.disconnect()
                            error("gateway_test: dropped mid-session")
                        end,
                    }
                end
                error("gateway_test: simulated network failure")
            end,
        }, function()
            pcall(gateway.connect)
        end)

        assert.equals(false, gateway.is_connected())
    end)
end)
