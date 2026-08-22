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

describe("gateway intents", function()
    -- The bitmask reaches Discord in the IDENTIFY, and a missing bit is silent:
    -- the gateway simply never delivers those events. The default shipped as
    -- 37889, which is bit 10 (GUILD_MESSAGE_REACTIONS) where bit 9
    -- (GUILD_MESSAGES) was meant, so the bot received no guild message at all
    -- and worked only in DMs. Asserted through the payload rather than against
    -- the constant, so the number and its meaning cannot drift apart again.
    local BIT = {
        GUILDS = 0,
        GUILD_MESSAGES = 9,
        GUILD_MESSAGE_REACTIONS = 10,
        DIRECT_MESSAGES = 12,
        MESSAGE_CONTENT = 15,
    }

    local function identify_intents()
        local sent = nil
        with_gateway_env({
            clock = function() return 1000.0 end,
            connect = function()
                return {
                    send = function(_, raw)
                        local payload = cru.json.decode(raw)
                        if payload.op == 2 then sent = payload.d.intents end
                        return true
                    end,
                    close = function() end,
                    receive = function()
                        -- HELLO, then stop: IDENTIFY is sent before this returns again.
                        if sent == nil then return HELLO end
                        gateway.disconnect()
                        error("gateway_test: stopping the run")
                    end,
                }
            end,
        }, function()
            pcall(gateway.connect)
        end)
        return sent
    end

    local function has_bit(mask, bit)
        return math.floor(mask / (2 ^ bit)) % 2 == 1
    end

    it("asks for GUILD_MESSAGES, or the bot is deaf in every server", function()
        local intents = identify_intents()
        assert.equals(true, intents ~= nil)
        assert.equals(true, has_bit(intents, BIT.GUILD_MESSAGES))
    end)

    it("asks for the other three the plugin documents", function()
        local intents = identify_intents()
        assert.equals(true, has_bit(intents, BIT.GUILDS))
        assert.equals(true, has_bit(intents, BIT.DIRECT_MESSAGES))
        assert.equals(true, has_bit(intents, BIT.MESSAGE_CONTENT))
    end)
end)

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

describe("gateway retry budget", function()
    -- `M.connect` called `cru.retry` once, and its body never returns while a
    -- connection is healthy — so one budget covered the daemon's whole life.
    -- Every drop ever, including Discord's routine OP 7 RECONNECT, spent a
    -- slot permanently and the backoff never decayed. On the eleventh the
    -- service raised, and nothing restarts a raw `services = {}` entry: the
    -- bot went offline for good, weeks after anyone touched it.
    it("earns a fresh budget after each live connection", function()
        local dials = 0
        local sockets_seen = 0

        -- Every socket connects, then drops. With one lifetime budget this
        -- stops at eleven dials; with a per-outage budget it keeps going.
        local function flaky_socket()
            sockets_seen = sockets_seen + 1
            local receives = 0
            return {
                send = function() return true end,
                close = function() end,
                receive = function()
                    receives = receives + 1
                    if receives == 1 then return HELLO end
                    if receives == 2 then return READY end
                    error("gateway_test: connection dropped")
                end,
            }
        end

        with_gateway_env({
            clock = function() return 1000.0 end,
            connect = function()
                dials = dials + 1
                if dials > 14 then
                    -- Past anything a single ten-attempt budget could reach.
                    gateway.disconnect()
                    error("gateway_test: stopping the run")
                end
                return flaky_socket()
            end,
        }, function()
            pcall(gateway.connect)
        end)

        assert.equals(15, dials)
        assert.equals(true, sockets_seen > 11)
    end)

    -- The other half: a gateway that never answers must still give up, or the
    -- outer loop spins through ten instant failures forever.
    it("gives up when a cycle never reaches a live connection", function()
        local dials = 0

        with_gateway_env({
            clock = function() return 1000.0 end,
            connect = function()
                dials = dials + 1
                error("gateway_test: simulated unreachable gateway")
            end,
        }, function()
            pcall(gateway.connect)
        end)

        -- One budget's worth of attempts, then out — not an endless loop.
        assert.equals(11, dials)
    end)
end)
