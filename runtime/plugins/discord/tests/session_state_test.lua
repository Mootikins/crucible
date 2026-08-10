--- DM sessions outlive a daemon restart. Channel sessions do not.
---
--- The daemon half is already settled: `agent_manager/tests/revive_cold.rs`
--- creates a session, throws away every piece of in-memory state, and shows a
--- cold manager reviving it — provided its kiln is open, which the daemon does
--- for the kilns of registered projects. What was left was remembering the id
--- across the restart, which is this file's subject.
---
--- DMs only. A channel session lives 900 seconds, so persisting one buys at
--- most fifteen minutes of continuity for a write on every channel message; a
--- DM lives 24 hours and is the conversation somebody expects to still be
--- there tomorrow.

local sessions = require("sessions")

local STATE_FILE = "/state/discord/sessions.json"
local DM_SESSION_TTL = 86400

--- A session API that records what it was asked to do.
---
--- Ids carry the caller's `prefix` because the state file outlives each test —
--- it is one in-memory mock filesystem for the whole run, which is the point:
--- entries written by an earlier test are exactly the entries a restart would
--- find. Two tests minting "state-1" would make "is this the id I created?"
--- unanswerable.
local function recording_api(prefix)
    local calls = { created = {}, configured = 0, ended = {} }
    return calls, {
        create = function(o)
            table.insert(calls.created, o)
            return { id = prefix .. "-" .. #calls.created }
        end,
        configure_agent = function()
            calls.configured = calls.configured + 1
            return true, nil
        end,
        end_session = function(id) table.insert(calls.ended, id) end,
    }
end

--- `crucible.config`, `cru.sessions` and `cru.paths` are all absent from the
--- plugin test VM — the daemon registers them, the bare executor the test
--- runner builds does not. `cru.fs` *is* present, as the harness's in-memory
--- mock, so every round trip below is a real `fs.write` followed by a real
--- `fs.read` rather than a stub agreeing with itself.
local function with_env(cfg, session_api, fn)
    crucible = crucible or {}
    local had_config, had_sessions, had_paths = crucible.config, cru.sessions, cru.paths
    crucible.config = { get = function(key) return cfg[key] end }
    cru.sessions = session_api
    cru.paths = {
        state = function(plugin) return "/state/" .. plugin end,
        join = function(dir, name) return dir .. "/" .. name end,
    }

    local ok, err = pcall(fn)

    cru.paths = had_paths
    cru.sessions = had_sessions
    crucible.config = had_config
    if not ok then error(err) end
end

local function configured(extra)
    local cfg = {
        ["discord.kiln"] = "/tmp/kiln",
        ["discord.provider"] = "p",
        ["discord.model"] = "m",
    }
    for k, v in pairs(extra or {}) do cfg[k] = v end
    return cfg
end

--- The map key for a sender, harvested from a real save rather than assumed.
---
--- `sessions.lua` keys on `(channel_id, author_id)` and persists that key
--- verbatim. Restoring a session the tests never created means writing a file
--- under that key, and hardcoding its shape would leave this file green while
--- the restore path it exercises had become unreachable. `harvest_key` pins
--- the two together instead.
local function harvest_key()
    local _, api = recording_api("harvest")
    local key
    with_env(configured(), api, function()
        local id = sessions.get_or_create("dm-harvest", nil, "u-harvest")
        for k, entry in pairs(sessions.load_from(STATE_FILE)) do
            if entry.session_id == id then key = k end
        end
    end)
    return key
end

describe("DM session persistence", function()
    it("writes a DM session to the state file, and reads it back", function()
        local calls, api = recording_api("round-trip")
        with_env(configured(), api, function()
            local id = sessions.get_or_create("dm-round-trip", nil, "u-1")
            assert.truthy(id)
            assert.equals(1, #calls.created)

            assert.truthy(cru.fs.exists(STATE_FILE))

            local restored = sessions.load_from(STATE_FILE)
            local found
            for _, entry in pairs(restored) do
                if entry.session_id == id then found = entry end
            end
            assert.is_not_nil(found)
            assert.equals("read", found.tier)
            assert.is_nil(found.guild_id)
        end)
    end)

    -- The reason D8 is "DM sessions only": a channel session is gone in 900
    -- seconds, and writing the file on every channel message to remember it is
    -- the write traffic the decision declined to pay.
    it("leaves channel sessions out of the file", function()
        local _, api = recording_api("channel")
        with_env(configured(), api, function()
            local id = sessions.get_or_create("chan-not-persisted", "guild-1", "u-2")
            assert.equals("channel-1", id)

            for _, entry in pairs(sessions.load_from(STATE_FILE)) do
                assert.falsy(entry.session_id == id)
            end
        end)
    end)

    -- The point of the whole item: an id written before a restart is the id a
    -- later message reuses, instead of paying for a new session.
    it("reuses a restored session rather than creating another", function()
        local key = harvest_key()
        assert.equals("dm-harvest\0u-harvest", key)

        local calls, api = recording_api("cold")
        with_env(configured(), api, function()
            local restart_key = "dm-cold\0u-cold"
            sessions.save_to(STATE_FILE, {
                [restart_key] = {
                    session_id = "survivor-1",
                    last_active = os.time(),
                    tier = "read",
                    key = restart_key,
                },
            })

            sessions.restore()

            local id = sessions.get_or_create("dm-cold", nil, "u-cold")
            assert.equals("survivor-1", id)
            assert.equals(0, #calls.created)
        end)
    end)

    -- Reviving an id already past its TTL only to end it on the next message
    -- costs a daemon round trip and leaves `:discord status` counting sessions
    -- nobody can reach.
    it("drops entries already older than the DM TTL", function()
        local _, api = recording_api("misc")
        with_env(configured(), api, function()
            local key = "dm-expired\0u-expired"
            sessions.save_to(STATE_FILE, {
                [key] = {
                    session_id = "too-old",
                    last_active = os.time() - DM_SESSION_TTL - 1,
                    tier = "read",
                    key = key,
                },
            })

            assert.is_nil(sessions.load_from(STATE_FILE)[key])
        end)
    end)

    -- `load_from` drops whatever is already past the DM TTL, so a file that
    -- only ever learns when a session was *created* would discard a 24-hour
    -- conversation the first time the daemon restarted a day into it.
    it("persists the activity bump, not only the creation", function()
        local _, api = recording_api("bump")
        with_env(configured(), api, function()
            local id = sessions.get_or_create("dm-bump", nil, "u-bump")
            local key
            for k, entry in pairs(sessions.load_from(STATE_FILE)) do
                if entry.session_id == id then key = k end
            end
            assert.is_not_nil(key)
            local created_at = sessions.load_from(STATE_FILE)[key].last_active

            -- `sessions.lua` reads `os.time()` at each use, so moving the clock
            -- is how a unit test reaches an aged entry.
            local real_time = os.time
            os.time = function() return real_time() + 3600 end
            local ok, err = pcall(function()
                assert.equals(id, sessions.get_or_create("dm-bump", nil, "u-bump"))
            end)
            os.time = real_time
            if not ok then error(err) end

            assert.equals(created_at + 3600, sessions.load_from(STATE_FILE)[key].last_active)
        end)
    end)

    -- A corrupt blob costs the conversations it held, not the plugin's ability
    -- to answer the next message.
    it("starts empty on an unreadable file rather than raising", function()
        local _, api = recording_api("misc")
        with_env(configured(), api, function()
            cru.fs.write(STATE_FILE, "{not json")
            assert.deep_equal({}, sessions.load_from(STATE_FILE))

            local id = sessions.get_or_create("dm-after-corrupt", nil, "u-3")
            assert.truthy(id)
        end)
    end)

    -- The daemon registers `cru.paths`; nothing else does. A runtime without
    -- it must still route messages.
    it("keeps working where no paths module exists", function()
        local had_paths = cru.paths
        cru.paths = nil
        local calls, api = recording_api("cold")
        local ok, err = pcall(function()
            crucible = crucible or {}
            local had_config, had_sessions = crucible.config, cru.sessions
            crucible.config = { get = function(key) return configured()[key] end }
            cru.sessions = api
            local id = sessions.get_or_create("dm-no-paths", nil, "u-4")
            cru.sessions = had_sessions
            crucible.config = had_config
            assert.truthy(id)
            assert.equals(1, #calls.created)
        end)
        cru.paths = had_paths
        if not ok then error(err) end
    end)
end)
