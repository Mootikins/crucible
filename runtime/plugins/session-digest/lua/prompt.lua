--- session-digest extraction prompt + transcript helpers.
---
--- The prompt is sent verbatim as the user-turn payload to a throwaway
--- chat session. The model must reply with JSON only — no markdown fences,
--- no commentary. Schema is enforced by `cru.grammar.presets.json()` when
--- the backend supports it; otherwise we fall back to prompt discipline
--- (see init.lua for the pcall around set_session_grammar).

local M = {}

--- Verbatim extraction instructions, locked in by plan
--- thoughts/shared/plans/2026-05-11-wave-2-agent-learning.md
M.INSTRUCTIONS = [[
You are reading a completed Crucible session. Your job is to extract:
1. A concise digest of the session (topics, decisions, action items).
2. A list of distinct entities mentioned (people, projects, concepts, tools).

Return ONLY valid JSON matching this shape (no markdown fences, no commentary):

{
  "digest": {
    "summary": "2-4 sentence summary in past tense",
    "topics": ["topic-a", "topic-b"],
    "decisions": ["decision text 1", "..."],
    "action_items": ["follow-up text 1", "..."]
  },
  "entities": [
    {
      "name": "Canonical entity name",
      "type": "person | project | concept | tool",
      "aliases": ["alt name 1", "..."],
      "facts": ["fact 1", "fact 2"]
    }
  ]
}

Rules:
- Skip generic entities ("the user", "agent", "Claude").
- Entities mentioned only in passing without facts: skip them.
- Facts must be present in the transcript, not inferred.
- If the session yielded no extractable entities, return entities: [].
]]

--- Extract textual content from a message regardless of part shape.
---
--- Daemon's `cru.context.messages(id)` returns rows shaped roughly like
--- `{ role = "user"|"assistant"|..., content = "...", parts = {...} }`.
--- Older sessions may have `parts` as an array of `{ type, content }`
--- tables. We coalesce both into a single string so the prompt stays
--- backend-agnostic.
function M.message_to_text(msg)
    if not msg then return "" end
    if type(msg.content) == "string" and msg.content ~= "" then
        return msg.content
    end
    if type(msg.parts) == "table" then
        local chunks = {}
        for _, part in ipairs(msg.parts) do
            if type(part) == "table" then
                if type(part.text) == "string" then
                    chunks[#chunks + 1] = part.text
                elseif type(part.content) == "string" then
                    chunks[#chunks + 1] = part.content
                end
            elseif type(part) == "string" then
                chunks[#chunks + 1] = part
            end
        end
        return table.concat(chunks, "\n")
    end
    return ""
end

--- Render a single message line as "role: text", skipping empties.
local function render_line(msg)
    local text = M.message_to_text(msg)
    if text == "" then return nil end
    local role = msg.role or "unknown"
    return string.format("%s: %s", role, text)
end

--- Truncate transcript to the last N message rows.
---
--- We currently only support the `last_n_turns` strategy. The
--- `summary` strategy is reserved for a future commit (it would recurse
--- via cru.context.compact); plan §truncate_strategy lists it as an
--- alternative but isn't required for Wave 2.
function M.truncate(messages, strategy, n)
    strategy = strategy or "last_n_turns"
    n = n or 40
    if strategy ~= "last_n_turns" then
        -- Unknown strategy: behave as last_n_turns rather than erroring.
        -- Logging here would be nice but cru.log isn't always present.
        strategy = "last_n_turns"
    end
    local total = #messages
    if total <= n then return messages end
    local out = {}
    for i = total - n + 1, total do
        out[#out + 1] = messages[i]
    end
    return out
end

--- Build the user-turn payload: instructions + transcript.
---
--- We deliberately put the transcript AFTER the instructions so the
--- last thing the model sees is the data, not the schema; in our
--- experience that produces more reliable JSON.
function M.build(messages, opts)
    opts = opts or {}
    local strategy = opts.truncate_strategy or "last_n_turns"
    local n = opts.last_n_turns or 40
    local truncated = M.truncate(messages, strategy, n)

    local lines = {}
    for _, msg in ipairs(truncated) do
        local line = render_line(msg)
        if line then lines[#lines + 1] = line end
    end
    local transcript = table.concat(lines, "\n\n")
    if transcript == "" then transcript = "(no transcript content)" end

    return M.INSTRUCTIONS .. "\n\n--- TRANSCRIPT ---\n\n" .. transcript
end

return M
