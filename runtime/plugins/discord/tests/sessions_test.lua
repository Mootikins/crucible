--- Session creation refuses rather than half-succeeding.
---
--- Both behaviours below shipped without a test, and the review proved it by
--- reverting each one and watching the suite stay green. They are the two ways
--- a misconfigured daemon used to answer every message in a channel with the
--- same failure for the full session TTL, instead of once.

local sessions = require("sessions")

--- `config.get` reads `crucible.config.get("discord." .. key)` inside a pcall,
--- and the test VM has no `crucible.config` — same shape as `routing_test`.
--- `cru.sessions` is stubbed for the same reason: the plugin VM has the real
--- bridge, the test VM has nothing.
local function with_env(cfg, session_api, fn)
    crucible = crucible or {}
    local had_config = crucible.config
    local had_sessions = cru.sessions
    crucible.config = { get = function(key) return cfg[key] end }
    cru.sessions = session_api

    local ok, err = pcall(fn)

    cru.sessions = had_sessions
    crucible.config = had_config
    if not ok then error(err) end
end

--- A session API that records what it was asked to do.
local function recording_api(opts)
    local calls = { created = {}, configured = 0, ended = {} }
    return calls, {
        create = function(o)
            table.insert(calls.created, o)
            return { id = "chat-test" }
        end,
        -- The bridge returns `(result, err)` rather than raising — every
        -- `cru.sessions.*` method surfaces its error as a second return value
        -- (`DaemonSessionApi`), and `configure_agent` branches on `err`.
        configure_agent = function()
            calls.configured = calls.configured + 1
            if opts and opts.configure_fails then
                return nil, "no such provider"
            end
            return true, nil
        end,
        end_session = function(id) table.insert(calls.ended, id) end,
    }
end

describe("get_or_create", function()
    -- Without a kiln, `create_session` falls back to `crucible_home()`, which
    -- stages reflection proposals under `~/.crucible/.crucible/proposals/`
    -- where `cru proposals list` never looks. Refusing before creating
    -- anything is the difference between a clear error and a session that
    -- silently writes where nobody reads.
    it("refuses without a configured kiln, and creates nothing", function()
        local calls, api = recording_api()
        with_env({}, api, function()
            local id, err = sessions.get_or_create("chan-no-kiln", "g1")
            assert.is_nil(id)
            assert.truthy(err)
            assert.equals(0, #calls.created)
        end)
    end)

    it("passes the configured kiln through to create", function()
        local calls, api = recording_api()
        with_env({
            ["discord.kiln"] = "/tmp/kiln",
            ["discord.provider"] = "p",
            ["discord.model"] = "m",
        }, api, function()
            local id = sessions.get_or_create("chan-kiln", "g1")
            assert.equals("chat-test", id)
            assert.equals(1, #calls.created)
            assert.equals("/tmp/kiln", calls.created[1].kiln)
        end)
    end)

    -- A session whose agent could not be configured used to be cached anyway,
    -- so every message in that channel answered "NoAgentConfigured" for the
    -- full TTL. Ending it and refusing means the next message retries.
    it("ends an unconfigurable session instead of caching it", function()
        local calls, api = recording_api({ configure_fails = true })
        with_env({
            ["discord.kiln"] = "/tmp/kiln",
            ["discord.provider"] = "p",
            ["discord.model"] = "m",
        }, api, function()
            local id, err = sessions.get_or_create("chan-bad-agent", "g1")
            assert.is_nil(id)
            assert.truthy(err)
            assert.equals(1, #calls.ended)

            -- The failure was not cached: a second message tries again rather
            -- than returning the dead id.
            sessions.get_or_create("chan-bad-agent", "g1")
            assert.equals(2, #calls.created)
        end)
    end)

    -- Missing provider/model is the same class: refuse, do not cache.
    it("refuses when provider and model are unset", function()
        local calls, api = recording_api()
        with_env({ ["discord.kiln"] = "/tmp/kiln" }, api, function()
            local id, err = sessions.get_or_create("chan-no-provider", "g1")
            assert.is_nil(id)
            assert.truthy(err)
            assert.equals(1, #calls.ended)
        end)
    end)
end)

-- One bot instance, different capability per requester. The tier is derived
-- from the two identities Discord gives us — the guild, or the account in a DM
-- — and it is fixed when the channel's session is created, which is why it must
-- be keyed on something stable for that channel rather than on the individual
-- message.
describe("access tiers", function()
    local function tier_with(access, guild_id, author_id)
        local result
        with_env({ ["discord.access"] = access }, {}, function()
            result = sessions.access_tier(guild_id, author_id)
        end)
        return result
    end

    it("defaults to read when nothing is configured", function()
        assert.equals("read", tier_with(nil, "g1", "u1"))
    end)

    it("gives a named account its tier in a DM", function()
        assert.equals("write", tier_with({ ["user:u1"] = "write" }, nil, "u1"))
    end)

    it("gives a named guild its tier", function()
        assert.equals("write", tier_with({ ["guild:g1"] = "write" }, "g1", "u1"))
    end)

    -- A guild message takes the guild's tier, not the sender's: everyone in a
    -- channel shares one session, so a per-account grant there would leak to
    -- whoever else is in the room.
    it("uses the guild tier for a guild message even when the author has one", function()
        local access = { ["guild:g1"] = "read", ["user:u1"] = "write" }
        assert.equals("read", tier_with(access, "g1", "u1"))
    end)

    it("honours an explicit default", function()
        assert.equals("write", tier_with({ default = "write" }, "g9", "u9"))
    end)

    it("falls back to read on an unknown tier name", function()
        assert.equals("read", tier_with({ ["user:u1"] = "superuser" }, nil, "u1"))
    end)

    it("gives a write tier the write tools and a read tier only reads", function()
        local seen = {}
        local calls, api = recording_api()
        api.configure_agent = function(_, cfg)
            table.insert(seen, cfg.tool_policy)
            return true, nil
        end
        local cfg = {
            ["discord.kiln"] = "/tmp/kiln",
            ["discord.provider"] = "p",
            ["discord.model"] = "m",
            ["discord.access"] = { ["user:writer"] = "write" },
        }
        with_env(cfg, api, function()
            sessions.get_or_create("dm-writer", nil, "writer")
            sessions.get_or_create("dm-reader", nil, "reader")
        end)

        assert.equals("allow", seen[1].write_file)
        assert.equals("allow", seen[1].read_file)
        assert.is_nil(seen[2].write_file)
        assert.equals("allow", seen[2].read_file)
        assert.equals(2, #calls.created)
    end)
end)
