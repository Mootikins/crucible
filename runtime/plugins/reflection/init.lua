--- reflection — a second self-improvement avenue, alongside knowledge insertion.
---
--- Knowledge insertion is reactive: the agent writes notes mid-turn when it
--- thinks to. The reflection pass is retrospective: after a session ends, a
--- forked cheap-model subagent reviews the finished conversation and *proposes*
--- new kiln notes. Proposals land in KILN/.crucible/proposals/ — outside the
--- indexed kiln — so a human disposes of them (`cru proposals ...`). Nothing is
--- ever auto-merged into the live graph. Propose, don't dispose.
---
--- Configure in init.lua:
---
---   require("reflection").setup({
---     model = "claude-haiku-4-5-20251001",  -- cheap aux model
---     min_turns = 3,                         -- skip trivial sessions
---     max_proposals = 5,
---     timeout = 120,
---   })
---
--- Or via TOML:
---
---   [plugins.reflection]
---   model = "claude-haiku-4-5-20251001"
---   enabled = true

local config = require("config")

local M = {}

-- ============================================================================
-- Reflection prompt (Hermes-style guardrails)
-- ============================================================================
--
-- The DO-NOT-capture list is the anti-pollution core, ported from Nous
-- Research's Hermes Agent. Without it, reflection floods the kiln with
-- environment noise and negative tool claims that are useless as durable
-- knowledge. The framing is propose-only: "Nothing to save" is a valid, common
-- outcome — the pass should be conservative.

M.SYSTEM_PROMPT = [==[
You are a reflection reviewer. You read a finished agent session and propose
durable knowledge notes worth keeping in a personal knowledge base (a "kiln").

You do NOT write to the kiln. You only PROPOSE. A human reviews every proposal.
Be conservative: proposing nothing is the correct answer for most sessions.

Capture ONLY durable, reusable knowledge, such as:
- A stable fact about this project/codebase that will still be true next week.
- A reusable technique, workflow, or decision with lasting rationale.
- A relationship between concepts worth linking in the graph.

DO NOT capture (these pollute the knowledge base):
- Environment-dependent failures: missing binaries, unconfigured credentials,
  wrong working directory, machine-specific paths.
- Negative claims about tools ("X is broken", "the API doesn't work") — these
  are usually transient or environment-specific, not durable knowledge.
- Transient errors that resolved themselves on retry.
- One-off task narratives ("I did X then Y then Z for this request").
- Secrets, tokens, credentials, or personal data.

Emit ONLY a JSON array (no prose, no code fences). Each element:
  {
    "title": "Short note title",
    "body": "Markdown note body. Use [[wikilinks]] to related concepts.",
    "tags": ["optional", "tags"],
    "target": "optional/relative/path.md"   // where it should land if accepted
  }
If nothing is worth saving, emit exactly: []
]==]

-- Sentinel baked into a reflection subagent's system prompt so the handler can
-- recognise (and skip) reflection-created sessions when they end. An HTML
-- comment so it is inert if the model echoes it.
M.REFLECTION_MARKER = "<!-- crucible:reflection-pass -->"

-- ============================================================================
-- Pure helpers (unit-testable without a daemon)
-- ============================================================================

--- True when a session's agent carries the reflection marker in its system
--- prompt — i.e. it is a reflection-created aux session. Reads `system_prompt`
--- defensively (the session object may not expose it in every context).
function M.is_reflection_session(session)
    if not session then return false end
    local ok, prompt = pcall(function() return session.system_prompt end)
    return ok and type(prompt) == "string"
        and prompt:find(M.REFLECTION_MARKER, 1, true) ~= nil
end

--- Count user turns in a message list. Trivial sessions (few turns) are
--- skipped, so this drives the min_turns gate.
function M.count_user_turns(messages)
    local n = 0
    for _, msg in ipairs(messages or {}) do
        if msg.role == "user" then
            n = n + 1
        end
    end
    return n
end

--- Render the message list as a plain-text transcript for the reviewer.
function M.build_transcript(messages)
    local parts = {}
    for _, msg in ipairs(messages or {}) do
        local role = msg.role or "unknown"
        local content = msg.content or ""
        parts[#parts + 1] = string.format("## %s\n%s", role, content)
    end
    return table.concat(parts, "\n\n")
end

--- Parse the reviewer's output into a proposal array. Tolerates surrounding
--- whitespace and accidental code fences. Returns a table (possibly empty) or
--- nil if the output is not valid JSON.
function M.parse_proposals(text)
    if not text or text == "" then return {} end

    -- Strip a leading/trailing ```json ... ``` fence if the model added one.
    local stripped = text:gsub("^%s*```%w*%s*", ""):gsub("%s*```%s*$", "")

    local ok, parsed = pcall(cru.json.decode, stripped)
    if not ok or type(parsed) ~= "table" then
        return nil
    end

    -- Tolerate a single proposal object instead of an array: a model that
    -- emits `{ "title": ..., "body": ... }` rather than `[ ... ]` should not be
    -- silently dropped. Detect by the presence of proposal keys with no array
    -- part.
    if #parsed == 0 and (parsed.title or parsed.body) then
        return { parsed }
    end
    return parsed
end

--- Slugify a title into a filesystem-safe fragment.
local function slugify(title)
    local slug = (title or ""):lower()
    slug = slug:gsub("[^%w]+", "-"):gsub("^%-+", ""):gsub("%-+$", "")
    if slug == "" then slug = "note" end
    return slug:sub(1, 48)
end

--- Deterministic-ish id for a proposal file (stem, no extension).
function M.proposal_id(proposal, index, stamp)
    stamp = stamp or os.date("!%Y%m%d-%H%M%S")
    return string.format("reflection-%s-%d-%s", stamp, index, slugify(proposal.title))
end

--- Escape a scalar for single-line YAML.
local function yaml_scalar(s)
    return '"' .. tostring(s):gsub('"', '\\"') .. '"'
end

--- Build a staged proposal file: provenance frontmatter + body. The
--- frontmatter carries `source`, `status`, `session`, `created` (stripped on
--- accept) plus the user-facing `title`/`tags`/`target`.
function M.render_proposal(proposal, session_id, stamp)
    stamp = stamp or os.date("!%Y-%m-%dT%H:%M:%SZ")
    local lines = {
        "---",
        "source: reflection",
        "status: proposed",
        "session: " .. yaml_scalar("[[" .. tostring(session_id) .. "]]"),
        "created: " .. yaml_scalar(stamp),
        "title: " .. yaml_scalar(proposal.title or "Untitled"),
    }
    if proposal.target and proposal.target ~= "" then
        lines[#lines + 1] = "target: " .. yaml_scalar(proposal.target)
    end
    if type(proposal.tags) == "table" and #proposal.tags > 0 then
        lines[#lines + 1] = "tags:"
        for _, tag in ipairs(proposal.tags) do
            lines[#lines + 1] = "  - " .. yaml_scalar(tag)
        end
    end
    lines[#lines + 1] = "---"
    lines[#lines + 1] = ""
    lines[#lines + 1] = proposal.body or ""
    return table.concat(lines, "\n")
end

-- ============================================================================
-- Orchestration (daemon-facing)
-- ============================================================================

local function collect_text(iter)
    local parts = {}
    if iter then
        while true do
            local part = iter()
            if not part then break end
            if part.type == "text" then
                parts[#parts + 1] = part.content
            end
        end
    end
    return table.concat(parts, "")
end

--- Write staged proposals to KILN/.crucible/proposals/ via cru.fs. We write the
--- files directly (not via the create_note tool) precisely because the staging
--- area must NOT be indexed — create_note targets the kiln index.
function M.stage_proposals(kiln, session_id, proposals)
    local dir = kiln .. "/.crucible/proposals"
    cru.fs.mkdir(dir)
    local written = {}
    local max = config.get("max_proposals", 5)
    for i, proposal in ipairs(proposals) do
        if i > max then break end
        if type(proposal) == "table" and proposal.title and proposal.body then
            local id = M.proposal_id(proposal, i)
            local path = dir .. "/" .. id .. ".md"
            cru.fs.write(path, M.render_proposal(proposal, session_id, nil))
            written[#written + 1] = path
        end
    end
    return written
end

--- The on_session_end handler. Reviews the finished session and stages
--- proposals. Best-effort: any failure is logged, never raised (a reflection
--- error must not disrupt session teardown).
function M.run(session)
    if not config.get("enabled", true) then return end
    if not session or not session.id then return end
    local session_id = session.id

    -- Recursion guard: ending our own aux session fires on_session_end again.
    -- min_turns alone is not a guarantee (a user may set min_turns = 1), so we
    -- positively identify reflection-created sessions by the marker baked into
    -- their system prompt and skip them regardless of turn count. This is a
    -- cross-session-visible tag (the aux session's config lives daemon-side),
    -- which module-level Lua state cannot be — each session has its own VM.
    if M.is_reflection_session(session) then
        cru.log("debug", "reflection: skipping reflection-created aux session")
        return
    end

    -- Resolve the aux model first: without it there is nothing to do, so bail
    -- before spending any RPC round-trips on the session's kiln and messages.
    local model = config.get("model", nil)
    if not model then
        cru.log("warn", "reflection: no aux model configured; skipping (set plugins.reflection.model)")
        return
    end

    local info = cru.sessions.get(session_id)
    local kiln = info and info.kiln
    if not kiln then
        cru.log("debug", "reflection: session has no kiln; skipping")
        return
    end

    local messages = cru.sessions.messages(session_id, {})
    if not messages then return end

    local turns = M.count_user_turns(messages)
    local min_turns = config.get("min_turns", 3)
    if turns < min_turns then
        cru.log("debug", string.format(
            "reflection: %d turns < min_turns %d; skipping", turns, min_turns))
        return
    end

    -- Fork a separate session for the review so it never touches the source
    -- session's prompt cache. It is NOT kiln-less: cru.sessions.create with no
    -- kiln defaults to the crucible home, so the marker-based recursion guard
    -- above (not the absence of a kiln) is what stops it reflecting on itself.
    local aux, err = cru.sessions.create({ type = "chat" })
    if err or not aux then
        cru.log("warn", "reflection: failed to create aux session: " .. tostring(err))
        return
    end

    local agent_cfg = {
        model = model,
        -- The marker tags this as a reflection session so its own teardown is
        -- skipped by the recursion guard.
        system_prompt = M.REFLECTION_MARKER .. "\n\n" .. M.SYSTEM_PROMPT,
        -- Bound the reviewer's tool-loop depth (it should emit JSON, not loop).
        max_iterations = config.get("max_iterations_cap", 3),
        -- No external tool servers for an unattended reviewer. NOTE: built-in
        -- tools are still attached — fully disabling tools needs a core knob
        -- (follow-up); low max_iterations + the propose-only prompt bound it.
        mcp_servers = {},
    }
    local provider = config.get("provider", nil)
    if provider then agent_cfg.provider = provider end
    cru.sessions.configure_agent(aux.id, agent_cfg)

    local transcript = M.build_transcript(messages)
    local prompt = "Review this finished session and propose durable notes.\n\n" .. transcript

    local iter, send_err = cru.sessions.send_and_collect(
        aux.id, prompt, { timeout = config.get("timeout", 120) })
    if send_err then
        cru.log("warn", "reflection: review failed: " .. tostring(send_err))
        cru.sessions.end_session(aux.id)
        return
    end

    local output = collect_text(iter)
    cru.sessions.end_session(aux.id)

    local proposals = M.parse_proposals(output)
    if proposals == nil then
        cru.log("warn", "reflection: could not parse reviewer output as JSON")
        return
    end
    if #proposals == 0 then
        cru.log("info", "reflection: nothing worth proposing")
        return
    end

    local written = M.stage_proposals(kiln, session_id, proposals)
    cru.log("info", string.format(
        "reflection: staged %d proposal(s) in %s/.crucible/proposals/", #written, kiln))
end

-- ============================================================================
-- Plugin Spec
-- ============================================================================

crucible.on_session_end(function(session)
    local ok, err = pcall(M.run, session)
    if not ok then
        cru.log("error", "reflection: handler error: " .. tostring(err))
    end
end)

return {
    name = "reflection",
    version = "0.1.0",
    description = "Retrospective self-improvement: propose kiln notes after a session ends",
    capabilities = { "session", "config" },

    -- Exposed for unit tests and manual invocation.
    run = M.run,
    is_reflection_session = M.is_reflection_session,
    reflection_marker = M.REFLECTION_MARKER,
    count_user_turns = M.count_user_turns,
    build_transcript = M.build_transcript,
    parse_proposals = M.parse_proposals,
    render_proposal = M.render_proposal,
    proposal_id = M.proposal_id,
    stage_proposals = M.stage_proposals,

    setup = function(cfg)
        if cfg then
            config.init(cfg)
        end
    end,
}
