--!strict
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

Read what the agent actually did, not only what it said. The transcript
includes tool calls and their results. A durable note explains WHY a command
sequence worked or failed, and gives the exact form that worked.

DO NOT capture (these pollute the knowledge base):
- Environment-dependent failures: missing binaries, unconfigured credentials,
  wrong working directory, machine-specific paths.
- Negative claims about tools ("X is broken", "the API doesn't work") — these
  are usually transient or environment-specific, not durable knowledge.
- Transient errors that resolved themselves on retry.
- One-off task narratives ("I did X then Y then Z for this request").
- Secrets, tokens, credentials, or personal data.

Before you propose, search the kiln with `semantic_search` for each idea and
read the closest note with `read_note`. If a note already covers it, propose
an UPDATE of that note with the new evidence folded in, not a new note.

Emit ONLY a JSON array (no prose, no code fences). Each element is one of:
  { "kind": "create", "title": "...", "body": "...", "tags": [...], "target": "optional/path.md" }
  { "kind": "update", "target": "existing/path.md", "title": "...", "body": "FULL revised note body" }
  { "kind": "skill",  "name": "lowercase-hyphens", "description": "one sentence: when to use it",
    "body": "SKILL.md body: When to apply, When NOT to apply, Steps. Wikilink the notes it rests on." }
Propose a skill only for a procedure the agent ran more than once, or that
failed once and then worked. Never propose an UPDATE to a skill: skills the
user installed are read-only to you, and a skill you wrote before gets a new
name, not an edit. If nothing is worth saving, emit exactly: []
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

--- The length of `text` in characters. Falls back to bytes when the text
--- is not valid UTF-8.
local function char_len(text: string): number
    return utf8.len(text) or #text
end

--- The first `n` characters of `text`. The cut lands on a glyph boundary:
--- a cut inside a multibyte glyph leaves bytes the daemon rejects as a
--- `String`, and the error would escape `M.run` before it ends the aux
--- session.
local function head(text: string, n: number): string
    local stop = utf8.offset(text, n + 1)
    if stop == nil then return text end
    return text:sub(1, stop - 1)
end

--- The last `n` characters of `text`. The cut lands on a glyph boundary,
--- for the same reason as `head`.
local function tail(text: string, n: number): string
    local start = utf8.offset(text, -n)
    if start == nil then return text end
    return text:sub(start)
end

--- Render the message list as a plain-text transcript for the reviewer.
--- Tool rows come from `cru.session.messages(id, { tools = true })`. Each
--- result is cut at `tool_result_chars`, and the whole transcript is cut at
--- `transcript_chars` from the front, so the reviewer sees how the session
--- ended. The WikiSkill maintainer caps each trace the same way. Both caps
--- count characters, not bytes.
export type TranscriptRow = {
    role: string?,
    content: string?,
    id: string?,
    name: string?,
    args: any,
    error: string?,
}

export type TranscriptOpts = {
    tool_result_chars: number?,
    transcript_chars: number?,
}

function M.build_transcript(messages: { TranscriptRow }?, opts: TranscriptOpts?): string
    local caps: TranscriptOpts = opts or {}
    local result_cap: number = caps.tool_result_chars or config.get("tool_result_chars", 2000)
    local total_cap: number = caps.transcript_chars or config.get("transcript_chars", 60000)
    local parts = {}
    for _, msg in ipairs(messages or {}) do
        local role = msg.role or "unknown"
        if role == "tool_call" then
            local args = msg.args
            if type(args) == "table" then args = cru.json.encode(args) end
            parts[#parts + 1] = string.format("## tool_call %s\n%s", tostring(msg.name), tostring(args or ""))
        elseif role == "tool_result" then
            local content = tostring(msg.content or "")
            local content_len = char_len(content)
            if content_len > result_cap then
                content = head(content, result_cap)
                    .. string.format("\n[truncated %d chars]", content_len - result_cap)
            end
            if msg.error and msg.error ~= "" then
                content = "ERROR: " .. tostring(msg.error) .. "\n" .. content
            end
            parts[#parts + 1] = "## tool_result\n" .. content
        else
            parts[#parts + 1] = string.format("## %s\n%s", role, tostring(msg.content or ""))
        end
    end
    local text = table.concat(parts, "\n\n")
    if char_len(text) > total_cap then
        text = "[earlier turns cut]\n\n" .. tail(text, total_cap)
    end
    return text
end

--- Parse the reviewer's output into a proposal array. Tolerates surrounding
--- whitespace and accidental code fences. Returns a table (possibly empty) or
--- nil if the output is not valid JSON.
function M.parse_proposals(text: string?): { any }?
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

--- Deterministic-ish id for a proposal file (stem, no extension). A skill
--- has a `name` and no `title`, so the name gives the slug.
function M.proposal_id(proposal, index, stamp)
    stamp = stamp or os.date("!%Y%m%d-%H%M%S")
    return string.format("reflection-%s-%d-%s", stamp, index, slugify(proposal.title or proposal.name))
end

--- True for a path a `create` or `update` may never touch. This mirrors
--- `is_skill_path` in `cru proposals`: any path component named `.crucible`
--- (not only the first one), or a file named `SKILL.md`. A proposal that
--- fails here is refused at accept time, so it is not worth a stage.
local function is_skill_path(target: string): boolean
    for component in target:gmatch("[^/]+") do
        if component == ".crucible" then return true end
    end
    return target:match("^SKILL%.md$") ~= nil or target:match("/SKILL%.md$") ~= nil
end

--- Return nil when a proposal is well formed, else a reason string. The
--- three kinds mirror `cru proposals`: a `create` lands a new note, an
--- `update` replaces the body of an existing note, a `skill` lands a
--- SKILL.md. Neither a create nor an update may land on a skill path:
--- installed skills are read-only to the reviewer.
function M.validate_proposal(p: any): string?
    if type(p) ~= "table" then return "not a table" end
    if type(p.body) ~= "string" or p.body == "" then return "missing body" end
    local kind = p.kind or "create"
    if kind == "create" then
        if type(p.title) ~= "string" or p.title == "" then return "missing title" end
        if type(p.target) == "string" and is_skill_path(p.target) then
            return "create may not target the .crucible directory or a SKILL.md"
        end
    elseif kind == "update" then
        if type(p.target) ~= "string" or p.target == "" then return "update needs a target" end
        if is_skill_path(p.target) then return "update may not target a skill" end
        if type(p.title) ~= "string" or p.title == "" then return "missing title" end
    elseif kind == "skill" then
        -- agentskills.io: 1 to 64 characters, a-z, 0-9 and single hyphens,
        -- with no hyphen at either end.
        local name = p.name
        local well_formed = type(name) == "string"
            and #name > 0
            and #name <= 64
            and name:match("^[a-z0-9][a-z0-9%-]*$") ~= nil
            and name:match("%-$") == nil
            and name:find("--", 1, true) == nil
        if not well_formed then return "skill needs a spec-valid name" end
        if type(p.description) ~= "string" or p.description == "" or #p.description > 1024 then
            return "skill needs a description of 1 to 1024 characters"
        end
        -- The CLI refuses a description that spans lines: the frontmatter
        -- splitter cuts at any line that starts with `---`.
        if p.description:find("[\n\r]") then return "skill description must be one line" end
    else
        return "unknown kind: " .. tostring(kind)
    end
    return nil
end

--- Escape a scalar for single-line YAML.
local function yaml_scalar(s)
    return '"' .. tostring(s):gsub('"', '\\"') .. '"'
end

--- Build a staged proposal file: provenance frontmatter + body. The
--- frontmatter carries `source`, `status`, `session`, `created` (stripped on
--- accept) plus the user-facing `title`/`tags`/`target`. `extra.model` names
--- the model that wrote the session; `cru proposals accept` keeps that line,
--- so a reader can tell one model's habits from another's in the kiln too.
--- A `kind` other than `create` is written out; a skill also carries its
--- `name` and `description`, and its title falls back to the name.
function M.render_proposal(proposal, session_id, stamp, extra)
    stamp = stamp or os.date("!%Y-%m-%dT%H:%M:%SZ")
    local lines = {
        "---",
        "source: reflection",
        "status: proposed",
        "session: " .. yaml_scalar("[[" .. tostring(session_id) .. "]]"),
        "created: " .. yaml_scalar(stamp),
    }
    local model = type(extra) == "table" and extra.model or nil
    if type(model) == "string" and model ~= "" then
        lines[#lines + 1] = "model: " .. yaml_scalar(model)
    end
    local kind = proposal.kind
    if type(kind) == "string" and kind ~= "create" then
        lines[#lines + 1] = "kind: " .. kind
    end
    if kind == "skill" then
        lines[#lines + 1] = "name: " .. tostring(proposal.name)
        lines[#lines + 1] = "description: " .. yaml_scalar(proposal.description)
    end
    lines[#lines + 1] = "title: " .. yaml_scalar(proposal.title or proposal.name or "Untitled")
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

--- The `title:` of each rejected proposal, newest first, at most `n`.
--- A proposal id starts with a UTC stamp, so a descending sort of the file
--- names is a sort by time. `files` maps file name to file content, so the
--- function needs no disk.
function M.rejected_titles(files: { [string]: string }?, n: number): { string }
    local out = {}
    if not files then return out end
    local names = {}
    for name, _ in pairs(files) do
        if name:match("%.md$") then names[#names + 1] = name end
    end
    table.sort(names, function(a, b) return a > b end)
    for _, name in ipairs(names) do
        if #out >= n then break end
        local text = files[name]
        local title = text:match("\ntitle:%s*\"(.-)\"%s*\n")
            or text:match("\ntitle:%s*(.-)%s*\n")
        if title and title ~= "" then out[#out + 1] = title end
    end
    return out
end

--- Read every file in the kiln's rejected/ directory into a name -> content
--- map. An absent directory is an empty map. A human who moves a file into
--- this directory by hand has rejected it just as well as the CLI.
function M.read_rejected(kiln: string): { [string]: string }
    local dir = cru.kiln.path(kiln, ".crucible/proposals/rejected")
    local files = {}
    if not cru.fs.is_dir(dir) then return files end
    for _, name in ipairs(cru.fs.list(dir)) do
        local handle = io.open(dir .. "/" .. name, "r")
        if handle then
            local text = handle:read("a")
            if text then files[name] = text end
            handle:close()
        end
    end
    return files
end

--- Compose the reviewer's user message from its parts. Every part is optional
--- except the transcript, so the tests can exercise one block at a time.
function M.build_prompt(parts: { transcript: string, rejected: { string }? }): string
    local sections = {}
    if parts.rejected and #parts.rejected > 0 then
        local lines = { "A human rejected these proposals recently; do not propose them again:" }
        for _, title in ipairs(parts.rejected) do
            lines[#lines + 1] = "- " .. title
        end
        sections[#sections + 1] = table.concat(lines, "\n")
    end
    sections[#sections + 1] = "Review this finished session and propose durable notes.\n\n" .. parts.transcript
    return table.concat(sections, "\n\n")
end

-- ============================================================================
-- Orchestration (daemon-facing)
-- ============================================================================

local function collect_text(iter: (() -> any)?): string
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

--- One plain file name: alphanumeric first character, then [%w._-], at
--- most 128 bytes. `cru.kiln.path` refuses traversal in the relative part
--- too; this keeps the plugin honest on its own, and a leading dot cannot
--- hide a proposal or spell "..".
function M.safe_filename(name)
    return type(name) == "string"
        and #name <= 128
        and name:match("^%w[%w._-]*$") ~= nil
end

--- Write staged proposals to KILN/.crucible/proposals/.
---
--- `kiln` is the kiln's NAME. `cru.kiln.path` turns the name into a directory,
--- so the plugin asks for the location instead of spelling one. We write the
--- files directly (not via the create_note tool) precisely because the staging
--- area must NOT be indexed — create_note targets the kiln index.
---
--- `cru.fs.mkdir` makes the directory; `io.open` writes the file. The write
--- does not go through `cru.fs.write`, which is being removed. `mkdir` stays
--- on `cru.fs` because Lua has no directory call, and `os.execute("mkdir -p")`
--- would interpolate a path into a command line.
function M.stage_proposals(kiln, session_id, proposals, extra)
    local dir = cru.kiln.path(kiln, ".crucible/proposals")
    cru.fs.mkdir(dir)
    local written = {}
    local max = config.get("max_proposals", 5)
    for i, proposal in ipairs(proposals) do
        if i > max then break end
        local why = M.validate_proposal(proposal)
        if why then
            cru.log("warn", "reflection: skipping proposal: " .. why)
        else
            local filename = M.proposal_id(proposal, i) .. ".md"
            if M.safe_filename(filename) then
                local path = dir .. "/" .. filename
                local handle, open_err = io.open(path, "w")
                if not handle then
                    error("reflection: cannot write " .. path .. ": " .. tostring(open_err))
                end
                handle:write(M.render_proposal(proposal, session_id, nil, extra))
                handle:close()
                written[#written + 1] = path
            end
        end
    end
    return written
end

--- The on_session_end handler. Reviews the finished session and stages
--- proposals. Answers with the paths it wrote; an empty list when it skipped
--- the session or the reviewer proposed nothing. Best-effort: any failure is
--- logged, never raised (a reflection error must not disrupt session teardown).
function M.run(session: any): { string }
    if not config.get("enabled", true) then return {} end
    if not session or not session.id then return {} end
    local session_id = session.id

    -- Recursion guard: ending our own aux session fires on_session_end again.
    -- min_turns alone is not a guarantee (a user may set min_turns = 1), so we
    -- positively identify reflection-created sessions by the marker baked into
    -- their system prompt and skip them regardless of turn count. This is a
    -- cross-session-visible tag (the aux session's config lives daemon-side),
    -- which module-level Lua state cannot be — each session has its own VM.
    if M.is_reflection_session(session) then
        cru.log("debug", "reflection: skipping reflection-created aux session")
        return {}
    end

    -- Resolve the aux model first: without it there is nothing to do, so bail
    -- before spending any RPC round-trips on the session's kiln and messages.
    local model = config.get("model", nil)
    if not model then
        cru.log("warn", "reflection: no aux model configured; skipping (set plugins.reflection.model)")
        return {}
    end

    -- The bridge sends kiln NAMES in a `kilns` array (session_bridge.rs).
    -- The first name is our staging target; cru.kiln.path resolves it.
    local info = cru.session.get(session_id)
    local kilns = info and info.kilns
    local kiln = kilns and kilns[1]
    if not kiln then
        cru.log("debug", "reflection: session has no kiln; skipping")
        return {}
    end

    local messages = cru.session.messages(session_id, { tools = true })
    if not messages then return {} end

    local turns = M.count_user_turns(messages)
    local min_turns = config.get("min_turns", 3)
    if turns < min_turns then
        cru.log("debug", string.format(
            "reflection: %d turns < min_turns %d; skipping", turns, min_turns))
        return {}
    end

    -- Every input the prompt needs comes before the aux session exists. A
    -- raise between `create` and `end_session` would leak the session: the
    -- handler's `pcall` only logs.
    local transcript = M.build_transcript(messages)
    local rejected = M.rejected_titles(M.read_rejected(kiln), config.get("rejection_memory", 20))
    local prompt = M.build_prompt({ transcript = transcript, rejected = rejected })

    local agent_cfg = {
        model = model,
        -- The marker tags this as a reflection session so its own teardown is
        -- skipped by the recursion guard.
        system_prompt = M.REFLECTION_MARKER .. "\n\n" .. M.SYSTEM_PROMPT,
        -- Bound the reviewer's tool-loop depth (it should emit JSON, not loop).
        max_iterations = config.get("max_iterations", 12),
        -- No external tool servers for an unattended reviewer. NOTE: built-in
        -- tools are still attached — fully disabling tools needs a core knob
        -- (follow-up); low max_iterations + the propose-only prompt bound it.
        mcp_servers = {},
    }
    local provider = config.get("provider", nil)
    if provider then agent_cfg.provider = provider end

    -- Fork a separate session for the review so it never touches the source
    -- session's prompt cache. The same kiln is attached, so the reviewer can
    -- call `semantic_search` and `read_note` before it proposes. The
    -- marker-based recursion guard above is what stops it reflecting on
    -- itself.
    local aux, err = cru.session.create({ type = "chat", kilns = { kiln } })
    if err or not aux then
        cru.log("warn", "reflection: failed to create aux session: " .. tostring(err))
        return {}
    end

    -- One `pcall` covers configure, send and collect, so the aux session
    -- ends on every path before the result is read.
    local ok, output = pcall(function(): string?
        cru.session.configure_agent(aux.id, agent_cfg)
        local iter, send_err = cru.session.send_and_collect(
            aux.id, prompt, { timeout = config.get("timeout", 120) })
        if send_err then
            cru.log("warn", "reflection: review failed: " .. tostring(send_err))
            return nil
        end
        return collect_text(iter)
    end)
    cru.session.end_session(aux.id)
    if not ok then
        cru.log("warn", "reflection: review raised: " .. tostring(output))
        return {}
    end
    if output == nil then return {} end

    local proposals = M.parse_proposals(output)
    if proposals == nil then
        cru.log("warn", "reflection: could not parse reviewer output as JSON")
        return {}
    end
    if #proposals == 0 then
        cru.log("info", "reflection: nothing worth proposing")
        return {}
    end

    local written = M.stage_proposals(kiln, session_id, proposals, { model = info and info.model })
    cru.log("info", string.format(
        "reflection: staged %d proposal(s) in the '%s' kiln's .crucible/proposals/",
        #written, kiln))
    return written
end

-- ============================================================================
-- Plugin Spec
-- ============================================================================

cru.on_session_end(function(session)
    -- Luau types `pcall` as `(boolean, R...)`, so `err` takes the type of
    -- what the function returns. The inner function drops the path list and
    -- returns nil, which gives `err` a slot the error message fits.
    local ok, err = pcall(function()
        M.run(session)
        return nil
    end)
    if not ok then
        cru.log("error", "reflection: handler error: " .. tostring(err))
    end
end)

local plugin = {
    name = "reflection",
    version = "0.1.0",
    description = "Retrospective self-improvement: propose kiln notes after a session ends",
    capabilities = { "kiln", "agent", "config", "filesystem" },

    -- Exposed for unit tests and manual invocation.
    run = M.run,
    is_reflection_session = M.is_reflection_session,
    reflection_marker = M.REFLECTION_MARKER,
    count_user_turns = M.count_user_turns,
    build_transcript = M.build_transcript,
    parse_proposals = M.parse_proposals,
    rejected_titles = M.rejected_titles,
    read_rejected = M.read_rejected,
    build_prompt = M.build_prompt,
    render_proposal = M.render_proposal,
    validate_proposal = M.validate_proposal,
    proposal_id = M.proposal_id,
    stage_proposals = M.stage_proposals,
    safe_filename = M.safe_filename,

    setup = function(cfg)
        if cfg then
            config.init(cfg)
        end
    end,
}

-- The daemon executes this file BY PATH, not through `require`, so nothing
-- would otherwise fill `package.loaded`. The documented
-- `require("reflection").setup{...}` from a user's init.lua would then load a
-- SECOND copy of this file: with its own upvalues, and re-running every
-- body-level `cru.on_*` call in it — so the `on_session_end` handler at :326 would be
-- registered twice and fire twice per event. Registering the spec here makes
-- that `require` answer with this table instead.
--
-- Same guard and same reason as `auto-title` and `web-search`. Enumerated
-- across every bundled plugin on 2026-08-19 after the auto-title fix stopped
-- at one plugin; `plugin_require_guard` pins the whole set.
package.loaded["reflection"] = plugin

return plugin
