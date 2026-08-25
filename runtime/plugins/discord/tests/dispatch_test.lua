--- The MESSAGE_CREATE path must reach `cru.timer.spawn`.
---
--- With no shims, a missed rename is a runtime nil-call, and the emitter
--- swallows it: the handler dies inside a pcall, the warning goes to a log no
--- test reads, and the bot goes silently deaf. This file drives one DM through
--- the real gateway receive loop and counts the spawns that LANDED — the
--- counter moves only after the real `cru.timer.spawn` accepted the task, so
--- neither a stale caller (`cru.spawn`) nor a missing registration can pass.

-- The runner VM has no cru.plugin (the daemon registers it); tests stub into it.
cru.plugin = cru.plugin or {}
local plugin = require("discord") -- registers the MESSAGE_CREATE handler
local gateway = require("gateway")
local config = require("config")

local function frame(payload)
    return { type = "text", data = cru.json.encode(payload) }
end

local HELLO = frame({ op = 10, d = { heartbeat_interval = 100 } })
local READY = frame({
    op = 0,
    t = "READY",
    s = 1,
    d = { session_id = "dispatch-test", user = { id = "bot-1", username = "test-bot" } },
})
local DM = frame({
    op = 0,
    t = "MESSAGE_CREATE",
    s = 2,
    d = { id = "m1", channel_id = "dm-dispatch", content = "hello there", author = { id = "u1" } },
})

describe("message dispatch", function()
    it("hands the responder turn to cru.timer.spawn", function()
        crucible = crucible or {}
        local cfg = {
            ["discord.allowed_users"] = { "u1" },
            ["discord.kiln"] = "notes",
            ["discord.provider"] = "p",
            ["discord.model"] = "m",
        }
        local had_config = cru.plugin.config
        local had_ws = cru.ws
        local had_sessions = cru.sessions
        local had_paths = cru.paths
        local had_clock = cru.timer.clock
        local had_spawn = cru.timer.spawn
        local had_get_token = config.get_token
        local had_random = math.random

        cru.plugin.config = { get = function(key) return cfg[key] end }
        config.get_token = function() return "dispatch-test-token" end
        -- Discord's first heartbeat is jittered by `math.random()`; pin it, and
        -- pin the clock, so no heartbeat tick fires during the scripted run.
        math.random = function() return 0.5 end
        cru.timer.clock = function() return 1000.0 end
        -- No persistence: `state_path` pcalls this and answers nil, so the run
        -- touches no disk.
        cru.paths = {
            state = function() error("dispatch_test: no persistence here") end,
        }

        -- Counts spawns that LANDED. `cru.timer` stays the REAL table and
        -- `real_spawn` the real function: the increment sits after the call,
        -- so a dead address raises inside the emitter's pcall and the counter
        -- stays 0.
        local real_spawn = cru.timer.spawn
        local landed = 0
        cru.timer.spawn = function(fn)
            local result = real_spawn(fn)
            landed = landed + 1
            return result
        end

        local created = {}
        cru.sessions = {
            create = function(opts)
                table.insert(created, opts)
                return { id = "dispatch-session-1" }
            end,
            configure_agent = function() return true, nil end,
            end_session = function() end,
        }

        local receives = 0
        cru.ws = {
            connect = function()
                return {
                    send = function() return true end,
                    close = function() end,
                    receive = function()
                        receives = receives + 1
                        if receives == 1 then return HELLO end
                        if receives == 2 then return READY end
                        if receives == 3 then return DM end
                        gateway.disconnect()
                        error("dispatch_test: stopping the receive loop")
                    end,
                }
            end,
        }

        local ok, err = pcall(gateway.connect)

        cru.ws = had_ws
        cru.sessions = had_sessions
        cru.paths = had_paths
        cru.timer.clock = had_clock
        cru.timer.spawn = had_spawn
        config.get_token = had_get_token
        math.random = had_random
        cru.plugin.config = had_config

        -- The handler crossed routing and quota, minted a session, and handed
        -- the responder turn to the real spawn. A swallowed nil-call leaves
        -- both at 0 — that, not the stop error, is the failure signal.
        expect.equals(1, #created)
        expect.equals(1, landed)
    end)
end)
