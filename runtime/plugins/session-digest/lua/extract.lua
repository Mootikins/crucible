--- LLM-driven digest extraction.
---
--- Runs ONE chat completion against a throwaway session, attempts to
--- constrain output via `cru.grammar.presets.json()` (Item 5; falls back
--- gracefully if the backend doesn't implement grammar — see header in
--- init.lua), then parses the response as JSON.
---
--- IMPORTANT (cost-control): every successful call here costs one LLM
--- request. Callers MUST opt-out before invoking this module — see
--- `init.lua`'s `on_session_end` orchestrator for the gating logic.

local prompt = require("lua.prompt")

local M = {}

--- Collect all text parts from a `send_and_collect` iterator into a
--- single string.
local function drain_text(response)
    if not response then return "" end
    local parts = {}
    while true do
        local part = response()
        if not part then break end
        -- send_and_collect yields rows shaped { type = "text"|..., content = "..." }
        -- We only care about "text" — thinking/tool_call/tool_result aren't
        -- part of the JSON answer.
        if type(part) == "table" and part.type == "text" then
            if type(part.content) == "string" then
                parts[#parts + 1] = part.content
            elseif type(part.text) == "string" then
                parts[#parts + 1] = part.text
            end
        end
    end
    return table.concat(parts, "")
end

--- Strip ```json ... ``` fences if the model added them despite
--- being instructed not to. Tolerates leading/trailing whitespace.
local function strip_fences(text)
    if not text or text == "" then return text end
    local trimmed = text:match("^%s*(.-)%s*$") or text
    -- Common shapes: ```json\n…\n```, ```\n…\n```
    local inner = trimmed:match("^```%w*%s*\n(.-)\n```$")
    if inner then return inner end
    inner = trimmed:match("^```%w*%s*(.-)```$")
    if inner then return inner end
    return trimmed
end

--- Validate the parsed extraction object against the locked schema.
---
--- Be lenient on missing fields: callers can recover from partial output
--- by filling in defaults, but a fundamentally wrong shape (e.g. raw
--- string, array at root) should surface as an error so the session
--- doesn't end up with junk notes.
local function validate(obj)
    if type(obj) ~= "table" then
        return false, "expected JSON object, got " .. type(obj)
    end
    if obj.digest ~= nil and type(obj.digest) ~= "table" then
        return false, "digest must be an object"
    end
    if obj.entities ~= nil and type(obj.entities) ~= "table" then
        return false, "entities must be an array"
    end
    return true, nil
end

--- Normalize a parsed extraction object so downstream callers can
--- assume all fields exist. Missing arrays become empty arrays;
--- missing summary becomes "".
local function normalize(obj)
    local digest = obj.digest or {}
    local entities = obj.entities or {}
    return {
        digest = {
            summary = digest.summary or "",
            topics = digest.topics or {},
            decisions = digest.decisions or {},
            action_items = digest.action_items or {},
        },
        entities = entities,
    }
end

--- Run extraction.
---
--- Args:
---   messages  array of conversation rows from cru.context.messages(id)
---   opts      { model?, truncate_strategy?, last_n_turns?, timeout? }
---   deps      injected dependencies for testability:
---             { sessions = cru.sessions, grammar = cru.grammar, json = cru.json }
---             Defaults to the real globals when nil.
---
--- Returns: parsed = { digest = {...}, entities = {...} }
---   or nil, error_string
function M.run(messages, opts, deps)
    opts = opts or {}
    deps = deps or {}
    local sessions = deps.sessions or (cru and cru.sessions)
    local grammar = deps.grammar or (cru and cru.grammar)
    local json = deps.json or (cru and cru.json)

    if not sessions or not sessions.create then
        return nil, "cru.sessions not available"
    end
    if not json or not json.decode then
        return nil, "cru.json not available"
    end

    -- Build prompt up-front so a transcript-construction failure doesn't
    -- waste a session create.
    local payload = prompt.build(messages, {
        truncate_strategy = opts.truncate_strategy,
        last_n_turns = opts.last_n_turns,
    })

    local create_opts = { type = "chat" }
    if opts.model and opts.model ~= "" then
        create_opts.model = opts.model
    end

    local session, err = sessions.create(create_opts)
    if err or not session then
        return nil, "session create failed: " .. tostring(err or "no session")
    end

    -- Attempt to attach the JSON grammar. set_session_grammar may not be
    -- supported by the active backend (BackendType::supports_grammar()
    -- currently returns false for every wired backend); guard with pcall
    -- so we degrade to prompt-only JSON discipline instead of crashing
    -- the whole digest pipeline. Logged for ops visibility.
    if grammar and grammar.presets and grammar.set_session_grammar then
        local ok_g, err_g = pcall(function()
            local g = grammar.presets.json()
            grammar.set_session_grammar(session.id, g)
        end)
        if not ok_g and cru and cru.log then
            cru.log("debug", "session-digest: grammar attach skipped: " .. tostring(err_g))
        end
    end

    local timeout = opts.timeout or 60
    local response, send_err = sessions.send_and_collect(
        session.id,
        payload,
        { timeout = timeout }
    )

    if send_err then
        if sessions.end_session then pcall(sessions.end_session, session.id) end
        return nil, "send failed: " .. tostring(send_err)
    end

    local raw = drain_text(response)
    if sessions.end_session then pcall(sessions.end_session, session.id) end

    local stripped = strip_fences(raw)
    if not stripped or stripped == "" then
        return nil, "empty response"
    end

    local ok, parsed = pcall(json.decode, stripped)
    if not ok or parsed == nil then
        return nil, "invalid JSON: " .. tostring(parsed or "decode failed")
    end

    local ok_v, why = validate(parsed)
    if not ok_v then
        return nil, why
    end

    return normalize(parsed), nil
end

-- Surface helpers for tests.
M._drain_text = drain_text
M._strip_fences = strip_fences
M._validate = validate
M._normalize = normalize

return M
