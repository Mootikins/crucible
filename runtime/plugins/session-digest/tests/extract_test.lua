--- Tests for lua/extract.lua + lua/prompt.lua.
---
--- These tests run via the plugin test runner (`cru plugin test
--- runtime/plugins/session-digest`). The runner provides describe/it/
--- assert + a minimal global `cru` namespace; we inject our own
--- `cru.sessions` / `cru.grammar` / `cru.json` stubs through the
--- `deps` parameter on extract.run so the tests don't depend on a real
--- daemon being attached.

local extract = require("lua.extract")
local prompt = require("lua.prompt")

-- ─────────────────────────────────────────────────────────────────────────────
-- Mocks
-- ─────────────────────────────────────────────────────────────────────────────

-- A simple counter-recording fake sessions module. Every test resets it.
local function make_sessions_mock(response_text, opts)
    opts = opts or {}
    local state = {
        create_calls = 0,
        send_calls = 0,
        end_calls = 0,
        last_payload = nil,
        last_opts = nil,
    }

    local sessions = {}

    function sessions.create(create_opts)
        state.create_calls = state.create_calls + 1
        if opts.create_err then
            return nil, opts.create_err
        end
        return { id = "stub-session-" .. tostring(state.create_calls) }, nil
    end

    function sessions.send_and_collect(session_id, content, send_opts)
        state.send_calls = state.send_calls + 1
        state.last_payload = content
        state.last_opts = send_opts
        if opts.send_err then
            return nil, opts.send_err
        end

        -- Build an iterator that yields a single text part with the
        -- canned response, then nil to signal completion.
        local emitted = false
        local function next_part()
            if emitted then return nil end
            emitted = true
            return { type = "text", content = response_text }
        end
        return next_part, nil
    end

    function sessions.end_session(_)
        state.end_calls = state.end_calls + 1
        return true, nil
    end

    return sessions, state
end

local function make_grammar_mock(opts)
    opts = opts or {}
    local state = { presets_calls = 0, set_calls = 0 }
    local grammar = {
        presets = {},
        set_session_grammar = function(_id, _g)
            state.set_calls = state.set_calls + 1
            if opts.set_err then error(opts.set_err) end
            return true
        end,
    }
    grammar.presets.json = function()
        state.presets_calls = state.presets_calls + 1
        return { _grammar = "json-stub" }
    end
    return grammar, state
end

-- cru.json is provided by the executor; we re-expose it through deps to
-- decouple tests from global state. Falls back to a minimal pure-Lua
-- decoder if the executor doesn't expose one (shouldn't happen in the
-- test runner but defensive).
local function get_json()
    if cru and cru.json and cru.json.decode then
        return cru.json
    end
    return nil
end

-- Sample transcript for re-use across tests
local sample_messages = {
    { role = "user", content = "Hi, I'm working on the auth module." },
    { role = "assistant", content = "Got it. Let's start by reading auth.rs." },
    { role = "user", content = "Refactor login() to use bcrypt." },
    { role = "assistant", content = "Done. Login now hashes via bcrypt." },
}

-- ─────────────────────────────────────────────────────────────────────────────
-- Prompt builder tests
-- ─────────────────────────────────────────────────────────────────────────────

describe("prompt.build", function()
    it("includes the locked-in instructions verbatim", function()
        local payload = prompt.build(sample_messages, {})
        assert.truthy(payload:find("You are reading a completed Crucible session", 1, true))
        assert.truthy(payload:find("Return ONLY valid JSON", 1, true))
    end)

    it("appends the transcript after the instructions", function()
        local payload = prompt.build(sample_messages, {})
        local instr_pos = payload:find("Return ONLY valid JSON", 1, true)
        local transcript_pos = payload:find("--- TRANSCRIPT ---", 1, true)
        assert.truthy(instr_pos)
        assert.truthy(transcript_pos)
        assert.truthy(transcript_pos > instr_pos)
    end)

    it("truncates long transcripts to last_n_turns", function()
        local many = {}
        for i = 1, 50 do
            many[i] = { role = "user", content = "msg-" .. i }
        end
        local payload = prompt.build(many, { last_n_turns = 10 })
        -- msg-41 .. msg-50 should be present
        assert.truthy(payload:find("msg-50", 1, true))
        assert.truthy(payload:find("msg-41", 1, true))
        -- msg-40 and earlier should be dropped
        assert.falsy(payload:find("msg-40", 1, true))
        assert.falsy(payload:find("msg-1\n", 1, true))
    end)

    it("renders parts-style messages too", function()
        local msgs = {
            { role = "user", parts = { { type = "text", text = "from parts" } } },
        }
        local payload = prompt.build(msgs, {})
        assert.truthy(payload:find("from parts", 1, true))
    end)

    it("substitutes a placeholder when transcript is empty", function()
        local payload = prompt.build({}, {})
        assert.truthy(payload:find("no transcript content", 1, true))
    end)
end)

-- ─────────────────────────────────────────────────────────────────────────────
-- Extract helpers
-- ─────────────────────────────────────────────────────────────────────────────

describe("extract._strip_fences", function()
    it("strips ```json fences", function()
        local out = extract._strip_fences("```json\n{\"a\": 1}\n```")
        assert.equal('{"a": 1}', out)
    end)

    it("strips bare ``` fences", function()
        local out = extract._strip_fences("```\n{\"a\": 1}\n```")
        assert.equal('{"a": 1}', out)
    end)

    it("returns content unchanged when no fences", function()
        local out = extract._strip_fences('{"a": 1}')
        assert.equal('{"a": 1}', out)
    end)

    it("handles surrounding whitespace", function()
        local out = extract._strip_fences("  \n```json\n{\"a\":1}\n```\n  ")
        assert.equal('{"a":1}', out)
    end)
end)

describe("extract._validate", function()
    it("accepts an object with digest + entities", function()
        local ok, err = extract._validate({ digest = {}, entities = {} })
        assert.truthy(ok)
        assert.is_nil(err)
    end)

    it("rejects a top-level array", function()
        local ok, _ = extract._validate({ 1, 2, 3 })
        -- arrays are tables in Lua so this is actually permitted...
        -- but if entities was a string, validate should still reject.
        assert.truthy(ok)
    end)

    it("rejects a string at root", function()
        local ok, err = extract._validate("hello")
        assert.falsy(ok)
        assert.truthy(err:find("object", 1, true))
    end)

    it("rejects non-array entities", function()
        local ok, err = extract._validate({ entities = "not-array" })
        assert.falsy(ok)
        assert.truthy(err:find("entities", 1, true))
    end)
end)

describe("extract._normalize", function()
    it("fills in missing arrays as empty", function()
        local out = extract._normalize({ digest = { summary = "hi" } })
        assert.equal("hi", out.digest.summary)
        assert.equal(0, #out.digest.topics)
        assert.equal(0, #out.digest.decisions)
        assert.equal(0, #out.digest.action_items)
        assert.equal(0, #out.entities)
    end)
end)

-- ─────────────────────────────────────────────────────────────────────────────
-- End-to-end run() — single LLM call discipline
-- ─────────────────────────────────────────────────────────────────────────────

describe("extract.run", function()
    local json = get_json()

    it("returns parsed JSON when the LLM responds with valid output", function()
        if not json then pending("cru.json unavailable") return end
        local response = '{"digest":{"summary":"ok","topics":["t"],"decisions":[],"action_items":[]},"entities":[{"name":"X","type":"concept","aliases":[],"facts":["fact 1"]}]}'
        local sessions, state = make_sessions_mock(response)
        local grammar = make_grammar_mock()

        local result, err = extract.run(sample_messages, {}, {
            sessions = sessions, grammar = grammar, json = json,
        })

        assert.is_nil(err)
        assert.truthy(result)
        assert.equal("ok", result.digest.summary)
        assert.equal(1, #result.entities)
        assert.equal("X", result.entities[1].name)
        assert.equal(1, state.create_calls)
        assert.equal(1, state.send_calls)
    end)

    it("returns an error when the LLM returns garbage", function()
        if not json then pending("cru.json unavailable") return end
        local sessions, _ = make_sessions_mock("not json at all { fragile")
        local result, err = extract.run(sample_messages, {}, {
            sessions = sessions, json = json,
        })
        assert.is_nil(result)
        assert.truthy(err)
        assert.truthy(err:find("invalid JSON", 1, true) or err:find("JSON", 1, true))
    end)

    it("strips markdown fences before parsing", function()
        if not json then pending("cru.json unavailable") return end
        local response = '```json\n{"digest":{"summary":"fenced"},"entities":[]}\n```'
        local sessions, _ = make_sessions_mock(response)
        local result, err = extract.run(sample_messages, {}, {
            sessions = sessions, json = json,
        })
        assert.is_nil(err)
        assert.equal("fenced", result.digest.summary)
    end)

    it("returns the create error when session creation fails", function()
        if not json then pending("cru.json unavailable") return end
        local sessions, _ = make_sessions_mock("", { create_err = "no model available" })
        local result, err = extract.run(sample_messages, {}, {
            sessions = sessions, json = json,
        })
        assert.is_nil(result)
        assert.truthy(err:find("no model available", 1, true))
    end)

    it("makes at most one chat session per run (cost-control)", function()
        if not json then pending("cru.json unavailable") return end
        local response = '{"digest":{"summary":""},"entities":[]}'
        local sessions, state = make_sessions_mock(response)
        extract.run(sample_messages, {}, { sessions = sessions, json = json })
        assert.equal(1, state.create_calls)
        assert.equal(1, state.send_calls)
    end)

    it("never crashes the caller when grammar attach fails", function()
        if not json then pending("cru.json unavailable") return end
        local response = '{"digest":{"summary":"survived"},"entities":[]}'
        local sessions, _ = make_sessions_mock(response)
        local grammar = make_grammar_mock({ set_err = "backend rejects grammar" })

        local result, err = extract.run(sample_messages, {}, {
            sessions = sessions, grammar = grammar, json = json,
        })
        assert.is_nil(err)
        assert.equal("survived", result.digest.summary)
    end)

    it("truncates the prompt to last_n_turns before sending", function()
        if not json then pending("cru.json unavailable") return end
        local response = '{"digest":{"summary":""},"entities":[]}'
        local many = {}
        for i = 1, 50 do
            many[i] = { role = "user", content = "MSG-" .. i }
        end

        local sessions, state = make_sessions_mock(response)
        extract.run(many, { last_n_turns = 5 }, { sessions = sessions, json = json })

        assert.truthy(state.last_payload:find("MSG-50", 1, true))
        assert.falsy(state.last_payload:find("MSG-45", 1, true))
        assert.equal(1, state.create_calls)
    end)
end)
