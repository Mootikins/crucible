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

--- Run `fn` with the clock moved forward, restoring it however `fn` ends.
---
--- Ages are the whole subject of the reply path's guards and of the sweep, and
--- moving the clock is the only way a unit test reaches them: `sessions.lua`
--- reads `os.time()` at each use rather than capturing it.
local function at_offset(seconds, fn)
    local real_time = os.time
    os.time = function() return real_time() + seconds end
    local ok, err = pcall(fn)
    os.time = real_time
    if not ok then error(err) end
end

--- A session API that records what it was asked to do.
local function recording_api(opts)
    local calls = { created = {}, configured = 0, ended = {} }
    return calls, {
        -- Each session gets its own id: sessions are keyed per sender now, so
        -- "did these two messages land in one session?" is the question most
        -- of these tests ask, and a constant id cannot answer it.
        create = function(o)
            table.insert(calls.created, o)
            return { id = "chat-" .. #calls.created }
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
            assert.equals("chat-1", id)
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
-- from the two identities Discord gives us — the account that sent the message
-- and the guild it came from — and it is fixed when that sender's session is
-- created, so every later message from them in that channel reuses it.
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

    -- The sender's own grant beats the guild's. This is the reverse of the
    -- rule that shipped first, and it is per-sender keying that makes it safe:
    -- a `user:` grant no longer leaks to whoever else is in the room, because
    -- nobody else is in that session. A guild key is now a floor for the
    -- unnamed, not a ceiling on the named.
    it("prefers the sender's own tier over the guild's", function()
        local access = { ["guild:g1"] = "read", ["user:u1"] = "write" }
        assert.equals("write", tier_with(access, "g1", "u1"))
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

-- A role names a principal the same way an account does: handing one out is a
-- deliberate act by whoever administers the server, and it is the grant that
-- keeps working when a new moderator arrives — no config edit, no restart.
-- Roles reach the plugin as `data.member.roles` on a guild MESSAGE_CREATE.
describe("role grants", function()
    local function tier_with(access, guild_id, author_id, roles)
        local result
        with_env({ ["discord.access"] = access }, {}, function()
            result = sessions.access_tier(guild_id, author_id, roles)
        end)
        return result
    end

    it("gives a member the tier their role names", function()
        assert.equals("write",
            tier_with({ ["role:r-mod"] = "write" }, "g1", "u1", { "r-mod" }))
    end)

    -- Same rule as `guild:`, one rung up: a grant aimed at this account beats
    -- one aimed at a group it happens to be in.
    it("prefers the sender's own tier over their role's", function()
        local access = { ["user:u1"] = "write", ["role:r-mod"] = "read" }
        assert.equals("write", tier_with(access, "g1", "u1", { "r-mod" }))
    end)

    it("prefers a role's tier over the guild's", function()
        local access = { ["guild:g1"] = "read", ["role:r-mod"] = "write" }
        assert.equals("write", tier_with(access, "g1", "u1", { "r-mod" }))
    end)

    -- Two granting roles on one member need a deterministic winner. The access
    -- map is a Lua table and `pairs` has no order, so the member's own role
    -- list is the only ordering either side of the comparison actually has.
    it("takes the first of the member's roles that names a tier", function()
        local access = { ["role:r-a"] = "read", ["role:r-b"] = "write" }
        assert.equals("write", tier_with(access, "g1", "u1", { "r-b", "r-a" }))
        assert.equals("read", tier_with(access, "g1", "u1", { "r-a", "r-b" }))
    end)

    it("skips roles that name no tier rather than stopping at them", function()
        local access = { ["role:r-mod"] = "write" }
        assert.equals("write",
            tier_with(access, "g1", "u1", { "r-none", "r-other", "r-mod" }))
    end)

    -- A DM carries no `member`, so there are no roles to read; and a role id is
    -- guild-scoped, so one carried in from elsewhere must not grant here.
    it("ignores roles in a DM", function()
        assert.equals("read",
            tier_with({ ["role:r-mod"] = "write" }, nil, "u1", { "r-mod" }))
    end)

    it("falls back to read on an unknown tier name from a role", function()
        assert.equals("read",
            tier_with({ ["role:r-mod"] = "superuser" }, "g1", "u1", { "r-mod" }))
    end)

    -- The tier is only worth resolving if it reaches the agent: `get_or_create`
    -- has to carry the roles from the event through to `configure_agent`.
    it("configures the agent from the role's tier", function()
        local seen
        local _, api = recording_api()
        api.configure_agent = function(_, cfg) seen = cfg.tool_policy; return true, nil end
        with_env({
            ["discord.kiln"] = "/tmp/kiln",
            ["discord.provider"] = "p",
            ["discord.model"] = "m",
            ["discord.access"] = { ["role:r-mod"] = "write", ["guild:g1"] = "read" },
        }, api, function()
            sessions.get_or_create("chan-roles", "g1", "u-mod", { roles = { "r-mod" } })
        end)

        assert.equals("allow", seen.write_file)
        assert.equals("allow", seen.read_file)
    end)
end)

-- The `ask` tier: the middle ground between granting a tool outright and
-- denying it with no recourse. It only works where exactly one identified
-- account can answer, which is a DM.
describe("the ask tier", function()
    local function tier_with(access, guild_id, author_id)
        local result
        with_env({ ["discord.access"] = access }, {}, function()
            result = sessions.access_tier(guild_id, author_id)
        end)
        return result
    end

    it("is handed out in a DM", function()
        assert.equals("ask", tier_with({ ["user:u1"] = "ask" }, nil, "u1"))
    end)

    -- Permissions are keyed on (session_id, permission_id) alone, so in a
    -- guild channel whoever replies first answers for everyone in the room.
    it("degrades to read in a guild rather than prompting a crowd", function()
        assert.equals("read", tier_with({ ["guild:g1"] = "ask" }, "g1", "u1"))
    end)

    it("marks only the ask tier as needing a live answer", function()
        assert.equals(true, sessions.tier_is_interactive("ask"))
        assert.equals(false, sessions.tier_is_interactive("write"))
        assert.equals(false, sessions.tier_is_interactive("read"))
    end)

    it("asks for the write tools and still allows reads outright", function()
        local seen
        local _, api = recording_api()
        api.configure_agent = function(_, cfg) seen = cfg.tool_policy; return true, nil end
        with_env({
            ["discord.kiln"] = "/tmp/kiln",
            ["discord.provider"] = "p",
            ["discord.model"] = "m",
            ["discord.access"] = { ["user:asker"] = "ask" },
        }, api, function()
            sessions.get_or_create("dm-asker", nil, "asker")
        end)

        assert.equals("ask", seen.write_file)
        assert.equals("ask", seen.create_note)
        -- Reads stay `allow`: prompting for every grep would make the tier
        -- unusable, and reads are what the read tier already grants freely.
        assert.equals("allow", seen.read_file)
        assert.equals("allow", seen.grep)
    end)
end)

-- Sessions are keyed on `(channel_id, author_id)`, not on the channel. A DM
-- channel is already per-account so DMs are unaffected; a guild channel now
-- gives each speaker their own session, which is what makes per-sender tiers,
-- quota attribution and one-prompt-per-person possible at all.
describe("per-sender keying", function()
    local function chat_cfg(access)
        return {
            ["discord.kiln"] = "/tmp/kiln",
            ["discord.provider"] = "p",
            ["discord.model"] = "m",
            ["discord.access"] = access,
        }
    end

    it("gives two speakers in one channel a session each", function()
        local calls, api = recording_api()
        with_env(chat_cfg(), api, function()
            local alice = sessions.get_or_create("chan-two", "g1", "alice")
            local bob = sessions.get_or_create("chan-two", "g1", "bob")
            assert.truthy(alice)
            assert.truthy(bob)
            assert.falsy(alice == bob)
            assert.equals(2, #calls.created)
        end)
    end)

    it("reuses one session for one account, message after message", function()
        local calls, api = recording_api()
        with_env(chat_cfg(), api, function()
            local first = sessions.get_or_create("chan-same", "g1", "alice")
            local second = sessions.get_or_create("chan-same", "g1", "alice")
            assert.equals(first, second)
            assert.equals(1, #calls.created)
        end)
    end)

    -- A DM channel holds exactly one account, so keying on the sender changes
    -- nothing there: the same channel still means the same session.
    it("leaves a DM unchanged", function()
        local calls, api = recording_api()
        with_env(chat_cfg(), api, function()
            local first = sessions.get_or_create("dm-solo", nil, "solo")
            local second = sessions.get_or_create("dm-solo", nil, "solo")
            assert.equals(first, second)
            assert.equals(1, #calls.created)
        end)
    end)

    it("counts one session per sender rather than one per channel", function()
        local before = sessions.active_count()
        local _, api = recording_api()
        with_env(chat_cfg(), api, function()
            sessions.get_or_create("chan-count", "g1", "alice")
            sessions.get_or_create("chan-count", "g1", "bob")
        end)
        assert.equals(before + 2, sessions.active_count())
    end)
end)

-- A Discord reply to something the bot said continues that message's session,
-- even when the bot said it to somebody else. `referenced_message` is on every
-- MESSAGE_CREATE, so this is deterministic and costs no model call.
describe("reply-chain continuity", function()
    local function chat_cfg(access)
        return {
            ["discord.kiln"] = "/tmp/kiln",
            ["discord.provider"] = "p",
            ["discord.model"] = "m",
            ["discord.access"] = access,
        }
    end

    --- One full turn: the sender's message routes to a session, and the bot's
    --- own reply echoes back over the gateway carrying the id of the message
    --- that triggered it — which is what indexes the bot message against the
    --- right session with no guessing between two speakers in one channel.
    ---
    --- Give every test its own message and bot ids: both indexes are module
    --- state that outlives a test, so a reused id can route this test's reply
    --- into the previous test's session and pass for the wrong reason.
    local function turn(channel, guild, author, msg_id, reply_to, bot_msg_id)
        local id = sessions.get_or_create(channel, guild, author, {
            message_id = msg_id,
            reply_to = reply_to,
        })
        if id and bot_msg_id then sessions.note_bot_message(bot_msg_id, msg_id) end
        return id
    end

    it("routes a reply into the session it replies to", function()
        local calls, api = recording_api()
        with_env(chat_cfg(), api, function()
            local alice = turn("chan-chain", "g1", "alice", "m1", nil, "bot1")
            local bob = turn("chan-chain", "g1", "bob", "m2", "bot1", nil)
            assert.equals(alice, bob)
            assert.equals(1, #calls.created)
        end)
    end)

    -- The chain extends: replying to the bot's answer *in* a joined session
    -- stays in it, rather than only the first hop working.
    it("keeps following the chain past the first hop", function()
        local calls, api = recording_api()
        with_env(chat_cfg(), api, function()
            local alice = turn("chan-chain2", "g1", "alice", "m1", nil, "bot1")
            turn("chan-chain2", "g1", "bob", "m2", "bot1", "bot2")
            local carol = turn("chan-chain2", "g1", "carol", "m3", "bot2", nil)
            assert.equals(alice, carol)
            assert.equals(1, #calls.created)
        end)
    end)

    -- The chain has to keep working on a session that is already warm, which is
    -- every guild turn after the first. The bot's answer to a *reused* session's
    -- message must be indexed exactly as the answer to its first one is —
    -- otherwise reply-continuity only ever works on a session's opening turn,
    -- which is the state nothing else in this file would notice.
    it("indexes the bot's answer on a session that was already warm", function()
        local calls, api = recording_api()
        with_env(chat_cfg(), api, function()
            local alice = turn("chan-warm", "g1", "alice", "warm-m1", nil, "warm-bot1")
            local again = turn("chan-warm", "g1", "alice", "warm-m2", nil, "warm-bot2")
            assert.equals(alice, again)
            local bob = turn("chan-warm", "g1", "bob", "warm-m3", "warm-bot2", nil)
            assert.equals(alice, bob)
            assert.equals(1, #calls.created)
        end)
    end)

    -- The replied-to session must still be warm by its own TTL. It is live here
    -- — nobody ended it — but a guild session goes cold after 15 minutes, and a
    -- reply arriving after that starts a turn in a session the daemon would have
    -- let lapse for its own sender.
    it("does not join a session that has gone cold", function()
        local calls, api = recording_api()
        local alice, bob
        with_env(chat_cfg(), api, function()
            alice = turn("chan-cold", "g1", "alice", "cold-m1", nil, "cold-bot1")
            at_offset(1000, function()
                bob = turn("chan-cold", "g1", "bob", "cold-m2", "cold-bot1", nil)
            end)
        end)
        assert.truthy(alice)
        assert.falsy(alice == bob)
        assert.equals(2, #calls.created)
    end)

    -- A session that has been ended and replaced must not be reachable through
    -- the index that still names it: replying to the old answer would resume an
    -- id the daemon has closed. Two guards stand behind this — the entry is no
    -- longer the live one for its sender, and its age is past the TTL — so
    -- neither is separately observable; the behaviour is what is pinned.
    it("does not resurrect a session that was ended and replaced", function()
        local calls, api = recording_api()
        local alice, alice2, bob
        with_env(chat_cfg(), api, function()
            alice = turn("chan-dead", "g1", "alice", "dead-m1", nil, "dead-bot1")
            at_offset(1000, function()
                alice2 = turn("chan-dead", "g1", "alice", "dead-m2", nil, "dead-bot2")
                bob = turn("chan-dead", "g1", "bob", "dead-m3", "dead-bot1", nil)
            end)
        end)
        assert.falsy(alice == alice2)
        assert.falsy(bob == alice)
        assert.falsy(bob == alice2)
        assert.equals(3, #calls.created)

        local ended = {}
        for _, id in ipairs(calls.ended) do ended[id] = true end
        assert.truthy(ended[alice])
    end)

    -- A turn whose answer never arrived must not stay pending forever: the
    -- dispatch record expires after five minutes, so a bot message echoing back
    -- long afterwards is attributed to nothing rather than to whatever session
    -- that message id happens to name. The session itself is still live and
    -- still warm here, so only the dispatch expiry can produce this.
    it("forgets a dispatch whose answer never came", function()
        local calls, api = recording_api()
        local alice, bob
        with_env(chat_cfg(), api, function()
            alice = sessions.get_or_create("chan-late", "g1", "alice", { message_id = "late-m1" })
            at_offset(400, function()
                sessions.cleanup_stale()
                sessions.note_bot_message("late-bot", "late-m1")
                bob = sessions.get_or_create("chan-late", "g1", "bob",
                    { message_id = "late-m2", reply_to = "late-bot" })
            end)
        end)
        assert.truthy(alice)
        assert.falsy(alice == bob)
        assert.equals(2, #calls.created)
    end)

    -- The reply must not hand the replier the tier the session was built with.
    -- `tool_policy` is fixed when the agent is configured, so joining would
    -- either grant a writer's session to a reader or the reverse; make theirs
    -- instead.
    it("refuses a reply from a sender of a different tier, and makes them their own", function()
        local calls, api = recording_api()
        local cfg = chat_cfg({ ["user:writer"] = "write" })
        with_env(cfg, api, function()
            local writer = turn("chan-tier", "g1", "writer", "m1", nil, "bot1")
            local reader = turn("chan-tier", "g1", "reader", "m2", "bot1", nil)
            assert.truthy(writer)
            assert.truthy(reader)
            assert.falsy(writer == reader)
            assert.equals(2, #calls.created)
        end)
    end)

    -- The index is in memory and unpersisted: a reply to a message the bot
    -- sent before a restart is simply a new turn.
    it("falls through to the sender's own session for an unknown message", function()
        local calls, api = recording_api()
        with_env(chat_cfg(), api, function()
            local own = turn("chan-unknown", "g1", "alice", "m1", nil, nil)
            local replied = turn("chan-unknown", "g1", "alice", "m2", "bot-from-before-the-restart", nil)
            assert.equals(own, replied)
            assert.equals(1, #calls.created)
        end)
    end)

    -- The echo carries no reference when the bot's message was not a reply, and
    -- an unrecognised trigger must index nothing rather than the wrong thing.
    it("indexes nothing when the bot message answers no known turn", function()
        local calls, api = recording_api()
        with_env(chat_cfg(), api, function()
            local alice = turn("chan-noref", "g1", "alice", "m1", nil, nil)
            sessions.note_bot_message("bot-orphan", nil)
            sessions.note_bot_message("bot-orphan2", "never-dispatched")
            local bob = turn("chan-noref", "g1", "bob", "m2", "bot-orphan", nil)
            local bob2 = turn("chan-noref", "g1", "bob", "m3", "bot-orphan2", nil)
            assert.falsy(alice == bob)
            assert.equals(bob, bob2)
            assert.equals(2, #calls.created)
        end)
    end)
end)

-- Stale sweeping walks the per-sender map, and must reach every sender in a
-- channel rather than the first one found under the channel id. Runs last: it
-- ends every session the file has created.
describe("cleanup_stale", function()
    it("ends every stale session, whichever sender it belongs to", function()
        local calls, api = recording_api()
        local real_time = os.time
        local alice, bob
        with_env({
            ["discord.kiln"] = "/tmp/kiln",
            ["discord.provider"] = "p",
            ["discord.model"] = "m",
        }, api, function()
            alice = sessions.get_or_create("chan-stale", "g1", "alice")
            bob = sessions.get_or_create("chan-stale", "g1", "bob")
            os.time = function() return real_time() + 1000000 end
            sessions.cleanup_stale()
            os.time = real_time
        end)
        os.time = real_time

        local ended = {}
        for _, id in ipairs(calls.ended) do ended[id] = true end
        assert.falsy(alice == bob)
        assert.truthy(ended[alice])
        assert.truthy(ended[bob])
        assert.equals(0, sessions.active_count())
    end)

    -- The sweep has to take the routing index with it. A DM is where that shows:
    -- the sweep ends a session after 2 hours idle while a DM session's own TTL
    -- is 24, so between those two an index entry naming an ended session is one
    -- the reply path would otherwise still call warm. Runs after the sweep test
    -- above because it ends every session again.
    it("leaves no reply route into a session it ended", function()
        local calls, api = recording_api()
        local before, after
        with_env({
            ["discord.kiln"] = "/tmp/kiln",
            ["discord.provider"] = "p",
            ["discord.model"] = "m",
        }, api, function()
            before = sessions.get_or_create("dm-swept", nil, "alice", { message_id = "swept-m1" })
            sessions.note_bot_message("swept-bot", "swept-m1")
            at_offset(7201, function()
                sessions.cleanup_stale()
                after = sessions.get_or_create("dm-swept", nil, "alice",
                    { message_id = "swept-m2", reply_to = "swept-bot" })
            end)
        end)

        local ended = {}
        for _, id in ipairs(calls.ended) do ended[id] = true end
        assert.truthy(ended[before])
        assert.falsy(before == after)
        assert.equals(2, #calls.created)
    end)
end)
