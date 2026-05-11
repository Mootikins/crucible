--- End-to-end tests for session-digest.
---
--- Drives `M.run_for_session` directly (not via the on_session_end hook
--- — that path is exercised by integration tests in the daemon crate,
--- not here). The pipeline is fed in-memory mocks for `cru.sessions`,
--- `cru.context`, `cru.kiln`, and `cru.grammar`, so the test stays
--- daemon-free.

local plugin = require("init")
local internal = plugin._internal
local config = internal.config

-- ─────────────────────────────────────────────────────────────────────────────
-- Test doubles
-- ─────────────────────────────────────────────────────────────────────────────

local function make_session(opts)
    opts = opts or {}
    return {
        id = opts.id or "session-abc12345-rest-of-uuid",
        kiln_path = opts.kiln_path,
        agent_name = opts.agent_name,
        end_reason = opts.end_reason,
    }
end

local function make_context_mock(messages)
    return {
        messages = function(_id, _opts)
            return messages, nil
        end,
    }
end

local function make_sessions_mock(response_text, ctrl)
    ctrl = ctrl or {}
    local state = { create_calls = 0, send_calls = 0, end_calls = 0 }
    local sessions = {}

    function sessions.create(_opts)
        state.create_calls = state.create_calls + 1
        if ctrl.create_err then return nil, ctrl.create_err end
        return { id = "spawned-" .. state.create_calls }, nil
    end

    function sessions.send_and_collect(_id, _content, _opts)
        state.send_calls = state.send_calls + 1
        if ctrl.send_err then return nil, ctrl.send_err end
        local emitted = false
        return function()
            if emitted then return nil end
            emitted = true
            return { type = "text", content = response_text }
        end, nil
    end

    function sessions.end_session(_id)
        state.end_calls = state.end_calls + 1
        return true
    end

    return sessions, state
end

local function make_kiln_mock(seed)
    seed = seed or {}
    local state = { notes = {}, create_log = {}, search_log = {} }
    for path, note in pairs(seed) do state.notes[path] = note end

    return {
        search = function(query, _)
            state.search_log[#state.search_log + 1] = query
            local results = {}
            local q = (query or ""):lower()
            for path, note in pairs(state.notes) do
                local canon = ((note.properties or {}).canonical_name or note.title or ""):lower()
                if canon ~= "" and canon:find(q, 1, true) then
                    results[#results + 1] = { path = path, title = note.title or path, score = 0.9 }
                end
            end
            return results
        end,
        get = function(path)
            local n = state.notes[path]
            if not n then return nil end
            local copy = {}
            for k, v in pairs(n) do copy[k] = v end
            return copy
        end,
        create_note = function(opts)
            state.create_log[#state.create_log + 1] = opts
            state.notes[opts.path] = {
                title = (opts.frontmatter or {}).canonical_name or opts.path,
                body = opts.body,
                properties = opts.frontmatter or {},
            }
            return "/abs/" .. opts.path, nil
        end,
    }, state
end

local function get_json()
    return cru and cru.json
end

local function build_deps(response_text, ctrl, messages, kiln_seed)
    local sessions, sstate = make_sessions_mock(response_text, ctrl)
    local kiln, kstate = make_kiln_mock(kiln_seed)
    return {
        sessions = sessions,
        context = make_context_mock(messages),
        json = get_json(),
        kiln = kiln,
    }, { sessions = sstate, kiln = kstate }
end

local SAMPLE_RESPONSE = '{"digest":{"summary":"Worked on auth.","topics":["auth"],"decisions":["use bcrypt"],"action_items":["unit tests"]},"entities":[{"name":"Crucible","type":"project","aliases":[],"facts":["the runtime"]},{"name":"bcrypt","type":"tool","aliases":[],"facts":["used for password hashing"]}]}'

local function long_messages(n)
    local out = {}
    for i = 1, n do
        out[i] = { role = i % 2 == 0 and "assistant" or "user", content = "turn-" .. i }
    end
    return out
end

-- ─────────────────────────────────────────────────────────────────────────────
-- should_run
-- ─────────────────────────────────────────────────────────────────────────────

describe("session-digest: should_run gating", function()
    before_each(function() config.reset() end)

    it("allows long enough, normal sessions", function()
        local ok, _ = internal.should_run(
            make_session({ end_reason = "user" }),
            long_messages(10)
        )
        assert.truthy(ok)
    end)

    it("skips when plugin is disabled in config", function()
        config.init({ enabled = false })
        local ok, reason = internal.should_run(make_session(), long_messages(10))
        assert.falsy(ok)
        assert.truthy(reason:find("disabled", 1, true))
    end)

    it("skips when session is shorter than min_session_turns", function()
        config.init({ min_session_turns = 5 })
        local ok, reason = internal.should_run(make_session(), long_messages(2))
        assert.falsy(ok)
        assert.truthy(reason:find("short", 1, true))
    end)

    it("skips when end_reason is error", function()
        local ok, reason = internal.should_run(
            make_session({ end_reason = "error" }),
            long_messages(10)
        )
        assert.falsy(ok)
        assert.truthy(reason:find("error", 1, true))
    end)

    it("skips when end_reason is timeout", function()
        local ok, reason = internal.should_run(
            make_session({ end_reason = "timeout" }),
            long_messages(10)
        )
        assert.falsy(ok)
        assert.truthy(reason:find("timeout", 1, true))
    end)
end)

-- ─────────────────────────────────────────────────────────────────────────────
-- run_for_session — happy path + cost control
-- ─────────────────────────────────────────────────────────────────────────────

describe("session-digest: run_for_session", function()
    before_each(function() config.reset() end)

    it("writes one digest note + one note per new entity", function()
        if not get_json() then pending("cru.json unavailable") return end
        local deps, state = build_deps(SAMPLE_RESPONSE, {}, long_messages(10), {})
        local result = internal.run_for_session(make_session({ end_reason = "user" }), deps)

        assert.truthy(result.digest_path)
        assert.is_nil(result.error)

        -- 1 digest + 2 entity writes = 3 create_note calls
        assert.equal(3, #state.kiln.create_log)

        local digest_call = state.kiln.create_log[1]
        assert.equal("session-digest", digest_call.frontmatter.type)
        assert.truthy(digest_call.path:find("Sessions/", 1, true))

        local entity_paths = {}
        for i = 2, 3 do
            entity_paths[#entity_paths + 1] = state.kiln.create_log[i].path
        end
        table.sort(entity_paths)
        assert.equal("Entities/Crucible.md", entity_paths[1])
        assert.equal("Entities/bcrypt.md", entity_paths[2])
    end)

    it("invokes the LLM AT MOST ONCE for a 50-turn session (cost-control)", function()
        if not get_json() then pending("cru.json unavailable") return end
        local deps, state = build_deps(SAMPLE_RESPONSE, {}, long_messages(50), {})
        internal.run_for_session(make_session({ end_reason = "user" }), deps)

        assert.equal(1, state.sessions.create_calls,
            "exactly one extraction session should be created")
        assert.equal(1, state.sessions.send_calls,
            "exactly one send_and_collect should happen")
    end)

    it("never invokes the LLM when plugin is disabled", function()
        if not get_json() then pending("cru.json unavailable") return end
        config.init({ enabled = false })
        local deps, state = build_deps(SAMPLE_RESPONSE, {}, long_messages(50), {})
        local result = internal.run_for_session(make_session({ end_reason = "user" }), deps)
        assert.truthy(result.skipped)
        assert.equal(0, state.sessions.create_calls)
        assert.equal(0, state.sessions.send_calls)
        assert.equal(0, #state.kiln.create_log)
    end)

    it("never invokes the LLM when session is too short", function()
        if not get_json() then pending("cru.json unavailable") return end
        config.init({ min_session_turns = 5 })
        local deps, state = build_deps(SAMPLE_RESPONSE, {}, long_messages(2), {})
        internal.run_for_session(make_session({ end_reason = "user" }), deps)
        assert.equal(0, state.sessions.create_calls)
        assert.equal(0, #state.kiln.create_log)
    end)

    it("never invokes the LLM when end_reason is error", function()
        if not get_json() then pending("cru.json unavailable") return end
        local deps, state = build_deps(SAMPLE_RESPONSE, {}, long_messages(50), {})
        internal.run_for_session(make_session({ end_reason = "error" }), deps)
        assert.equal(0, state.sessions.create_calls)
        assert.equal(0, #state.kiln.create_log)
    end)

    it("surfaces extraction errors without crashing", function()
        if not get_json() then pending("cru.json unavailable") return end
        local deps, _ = build_deps("not json", {}, long_messages(10), {})
        local result = internal.run_for_session(make_session({ end_reason = "user" }), deps)
        assert.truthy(result.error)
    end)

    it("merges into existing entity notes rather than duplicating", function()
        if not get_json() then pending("cru.json unavailable") return end
        local seed = {
            ["Entities/Crucible.md"] = {
                title = "Crucible",
                body = "# Crucible\n\n## Facts\n- the runtime (source: [[Sessions/old]])\n",
                properties = {
                    type = "entity",
                    canonical_name = "Crucible",
                    entity_type = "project",
                    aliases = {},
                },
            },
        }
        local deps, state = build_deps(SAMPLE_RESPONSE, {}, long_messages(10), seed)
        local result = internal.run_for_session(make_session({ end_reason = "user" }), deps)

        assert.is_nil(result.error)
        -- created: 1 digest + 1 new "bcrypt"; merged: existing Crucible
        assert.equal(1, #result.entities.created)
        assert.equal(1, #result.entities.merged)

        -- The merged note should still contain the original fact
        local merged_path = result.entities.merged[1]
        local note = deps.kiln.get(merged_path)
        assert.truthy(note.body:find("the runtime", 1, true))
    end)
end)
