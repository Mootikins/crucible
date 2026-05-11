--- session-digest — derive digests + entity notes from completed sessions.
---
--- Registers a `crucible.on_session_end` hook. When a session ends, the
--- handler runs ONE LLM extraction pass over the transcript, parses the
--- response as JSON, and writes one digest note plus one note per new
--- entity. Existing entities get merged-in facts (see lua/dedupe.lua).
---
--- Cost discipline (verification checklist):
---   • At most ONE chat-completion call per session, ever.
---   • Sessions shorter than `min_session_turns` skip extraction.
---   • `end_reason == "error" | "timeout"` skips extraction (partial
---     sessions aren't worth digesting).
---   • Plugin-level `config.enabled = false` short-circuits everything.
---
--- KNOWN LIMITATION — grammar fallback:
---   `BackendType::supports_grammar()` currently returns false for every
---   wired backend, so `cru.grammar.set_session_grammar` will hard-error
---   on a real backend. We attach the JSON grammar via `pcall` and fall
---   back to prompt-only JSON discipline when the call fails. See
---   lua/extract.lua for the pcall wrapper.
---
--- KNOWN LIMITATION — entity merge race:
---   Two simultaneous session-ends touching the same entity could clobber
---   each other's body. There's no `update_note` API yet; we use
---   `overwrite = true` with a fresh body. Documented in the Wave 2
---   plan's "Conflict resolution" section. Concurrent-extraction lock is
---   explicitly out of scope for Wave 2.
---
--- FUTURE WORK — per-session opt-out via frontmatter:
---   The plan calls for honoring `digest: false` in the session's
---   frontmatter, but Session userdata doesn't yet surface frontmatter.
---   For now only the global `config.enabled` flag and threshold checks
---   gate execution. Tracked in the same plan file.

local config = require("lua.config")
local extract = require("lua.extract")
local dedupe = require("lua.dedupe")
local write = require("lua.write")

local M = {}

-- ─────────────────────────────────────────────────────────────────────────────
-- Helpers
-- ─────────────────────────────────────────────────────────────────────────────

local function log(level, message)
    if cru and cru.log then
        pcall(cru.log, level, message)
    end
end

local function load_messages(session_id, deps)
    deps = deps or {}
    local context = deps.context or (cru and cru.context)
    if not context or not context.messages then return {}, "cru.context.messages unavailable" end

    local msgs, err = context.messages(session_id)
    if err then return nil, tostring(err) end
    return msgs or {}, nil
end

--- Decide whether to run extraction for this session.
---
--- Returns (true, nil) on go, (false, "reason") otherwise.
function M.should_run(session, messages, opts)
    opts = opts or {}
    if not config.get("enabled", true) then
        return false, "plugin disabled in config"
    end

    -- end_reason gating: skip partial sessions.
    local reason = session and session.end_reason
    if reason == "error" or reason == "timeout" then
        return false, "skip end_reason=" .. tostring(reason)
    end

    local min_turns = config.get("min_session_turns", 3)
    local turn_count = messages and #messages or 0
    if turn_count < min_turns then
        return false, string.format(
            "skip short session (%d turns < min %d)", turn_count, min_turns
        )
    end

    return true, nil
end

-- ─────────────────────────────────────────────────────────────────────────────
-- Orchestration
-- ─────────────────────────────────────────────────────────────────────────────

--- Run the full extract → dedupe → write pipeline for a session.
---
--- Exposed at the module level so tests can drive it without going
--- through the hook registration.
---
--- Returns a summary table:
---   { skipped = "reason" } if gated out
---   { digest_path = "...", entities = {...} } on success
---   { error = "..." } on failure
function M.run_for_session(session, deps)
    deps = deps or {}

    if not session or not session.id then
        return { error = "invalid session" }
    end

    local messages, msg_err = load_messages(session.id, deps)
    if msg_err then
        log("warn", "session-digest: load messages failed: " .. msg_err)
        return { error = msg_err }
    end

    local ok, reason = M.should_run(session, messages)
    if not ok then
        log("debug", "session-digest: " .. reason)
        return { skipped = reason }
    end

    local extraction, err = extract.run(messages, {
        model = config.get("model"),
        truncate_strategy = config.get("truncate_strategy", "last_n_turns"),
        last_n_turns = config.get("last_n_turns", 40),
    }, deps)
    if not extraction then
        log("warn", "session-digest: extraction failed: " .. tostring(err))
        return { error = tostring(err) }
    end

    local date = os.date("%Y-%m-%d")
    local digest_abs, digest_err, digest_rel = write.write_digest(
        extraction,
        { session_id = session.id, date = date },
        deps
    )
    if not digest_abs then
        log("warn", "session-digest: digest write failed: " .. tostring(digest_err))
        return { error = "digest write failed: " .. tostring(digest_err) }
    end

    local dedupe_summary = dedupe.process(
        extraction.entities or {},
        {
            source_wikilink = write.digest_wikilink(digest_rel or digest_abs),
            dedupe_threshold = config.get("dedupe_threshold", 0.80),
        },
        deps
    )

    log("info", string.format(
        "session-digest: wrote %s; entities created=%d merged=%d skipped=%d",
        digest_rel or digest_abs,
        #dedupe_summary.created,
        #dedupe_summary.merged,
        #dedupe_summary.skipped
    ))

    return {
        digest_path = digest_abs,
        digest_rel = digest_rel,
        entities = dedupe_summary,
    }
end

-- ─────────────────────────────────────────────────────────────────────────────
-- Hook registration
-- ─────────────────────────────────────────────────────────────────────────────

local function register_hook()
    if not crucible or not crucible.on_session_end then return end
    crucible.on_session_end(function(session)
        -- Errors inside this handler must not crash the daemon's session
        -- end path. The hook dispatch already isolates via tracing::error
        -- on Err, but we belt-and-suspender pcall so partial test setups
        -- (e.g. missing cru.kiln) degrade gracefully too.
        local ok, err_or_summary = pcall(M.run_for_session, session)
        if not ok then
            log("error", "session-digest: handler crashed: " .. tostring(err_or_summary))
        end
    end)
end

register_hook()

-- ─────────────────────────────────────────────────────────────────────────────
-- Plugin spec
-- ─────────────────────────────────────────────────────────────────────────────

return {
    name = "session-digest",
    version = "0.1.0",
    description = "Extract digests + entity notes from completed sessions",
    capabilities = { "kiln", "agent", "config" },

    setup = function(cfg)
        if cfg then config.init(cfg) end
    end,

    -- Test surface: expose internals so tests can drive the pipeline
    -- without re-firing `on_session_end` (which the test harness
    -- doesn't simulate). Also lets tests inject `deps` mocks.
    _internal = {
        config = config,
        extract = extract,
        dedupe = dedupe,
        write = write,
        run_for_session = M.run_for_session,
        should_run = M.should_run,
    },
}
