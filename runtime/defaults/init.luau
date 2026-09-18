--!strict
-- Crucible built-in Lua defaults
-- Loaded once, at daemon boot, before ~/.config/crucible/init.lua.
-- That file runs second on the same VM, so anything it assigns wins and
-- `cru.modes.x = nil` removes. Nothing merges and nothing re-applies.

-- Precognition context formatter
--
-- Customizes how relevant notes are injected before the FIRST LLM turn of a
-- session (precognition runs once, on the session's first user message).
-- Receives: ctx, event — one flat table, payload fields at the top level
-- (event.user_message, event.results, ...). Return a string to use as the
-- context block, or nil to fall back.
--
-- To change it, register your own handler for this event from a plugin.
cru.on("precognition_format", function(ctx, event)
  -- `event.payload` guard kept for sessions replaying pre-flattening
  -- recordings; live events are flat.
  local payload = event.payload or event
  local results = payload and payload.results

  if not results or #results == 0 then
    return nil
  end

  local lines = {}
  table.insert(lines, string.format("## Relevant Notes (%d)", #results))
  table.insert(lines, "")

  for _, note in ipairs(results) do
    local title = note.title or "Untitled"
    local score = tonumber(note.score) or 0
    local score_pct = math.floor(score * 100)

    table.insert(lines, string.format("### %s (%d%% match)", title, score_pct))

    if note.snippet and note.snippet ~= "" then
      table.insert(lines, note.snippet)
    end

    table.insert(lines, "")
  end

  return table.concat(lines, "\n")
end)

-- The built-in modes
--
-- A mode is two things: which tools it exposes, and what it does when one of
-- them needs permission. Both live here, so adding a mode is adding a line
-- rather than editing the daemon.
--
--   tools       "*" or a list of glob patterns ("read_*", "*_search")
--   permissions "ask" | "allow" | "deny", or a rules table
--
-- Tool patterns gate VISIBILITY — whether the model is offered the tool at
-- all. What may be done WITH a visible tool is a permission rule, in the same
-- `tool:pattern` grammar as the [permissions] config, evaluated by the same
-- engine. That is how you say "this mode may use bash, but only to search":
--
--   cru.modes.review = {
--     tools = { "read_*", "*_search", "bash" },
--     permissions = {
--       default = "deny",
--       allow = { "bash:rg *", "bash:grep *", "bash:fd *" },
--     },
--   }
--
-- Chained commands are handled by the engine, so `bash:rg *` does NOT admit
-- `rg foo && rm -rf /`.
--
-- These are declarations, not enforcement. The daemon independently filters
-- plan mode's advertised tool set and refuses plugin tools at dispatch, and
-- the things that must not be widenable — [permissions] deny rules, workspace
-- containment, the shell policy — are outside modes entirely and stay
-- unconditional. Widening a mode here cannot widen those.
--
-- Add or remove a mode from ~/.config/crucible/init.lua, which runs after
-- this file:
--
--   cru.modes.review = { tools = { "read_*", "*_search" }, permissions = "ask" }
--   cru.modes.auto   = nil    -- remove it entirely
--
-- For anything CONDITIONAL, a stance is not enough — use a hook, which runs
-- before the stance and wins:
--
--   cru.permissions.on_request(function(request)
--     if request.tool_name == "bash" then return { deny = true } end
--     return nil
--   end)
-- The id is the stance. "normal" said only "this is the default one", while
-- the mode's actual rule — ask before each change — was written one line
-- below it, and its old description ("Full read/write access") described
-- `auto`. Cursor and Claude Code both show this mode as "Ask"/"always ask".
-- The host resolves the former id `normal` to this one, so a session or
-- config written before the rename still loads.
cru.modes.ask = {
  label = "Ask",
  description = "Ask before each change",
  tools = "*",
  permissions = "ask",
}

-- Plan's stance is "ask", not "deny", because its real rule is CONDITIONAL
-- and a stance can only be static: plan forbids mutation, not reading, and an
-- agent card's `ask` policy can push a read-only tool through the gate. The
-- hook below expresses that. This is the split the two tiers exist for.
cru.modes.plan = {
  label = "Plan",
  description = "Read-only exploration",
  tools = {
    -- `grep_notes` is named explicitly: it is the one read-only search tool
    -- the `*_search` glob no longer covers since the text_search rename.
    "read_*", "list_*", "get_*", "*_search", "grep_notes", "glob", "grep",
    "skill_view",
  },
  permissions = "ask",
}

cru.modes.auto = {
  label = "Auto",
  description = "Auto-approve all operations",
  tools = "*",
  permissions = "allow",
}

-- Plan mode denies anything that could mutate
--
-- `request.is_safe` is the daemon's own read-only classification. Read-only
-- tools normally never reach the gate, but an agent card's `ask` policy can
-- force one through, and denying a read in plan mode would be wrong — which
-- is exactly why this is a hook and not `permissions = "deny"`.
--
-- The daemon enforces plan mode independently (it filters the advertised tool
-- set and refuses plugin tools at dispatch), so this is the legible statement
-- of the rule, not the thing holding it up.
--
-- This file loads before your own, so this hook is asked FIRST. It answers
-- `nil` for every mode but `plan`, which is what leaves the decision to the
-- hooks you register after it. In `plan` mode it decides, and a hook of yours
-- cannot allow a mutating tool through this gate.
cru.permissions.on_request(function(request)
  if request.mode == "plan" and not request.is_safe then
    return { deny = true }
  end
  return nil
end)

-- The default system prompt is NOT set here.
--
-- It ships as `chat.system_prompt` in the config store, on the Default layer.
-- A value written from this file would carry the Lua layer instead, which
-- outranks `settings.json`, so the settings UI could not change it.
--
-- To replace it, write the key from your ~/.config/crucible/init.lua:
--
--   cru.config.set { chat = { system_prompt = "…" } }
--
-- To extend it:
--
--   cru.config.set {
--     chat = {
--       system_prompt = cru.config.get("chat").system_prompt
--         .. "\n\nAnswer in British English.",
--     },
--   }
--
-- An agent card's own system_prompt wins over both.
--
-- For anything conditional, set it per session instead. `session.x` reads the
-- inherited value first, so this extends rather than replaces:
--
--   cru.on_session_start(function(session)
--     if session.workspace:match("/work/") then
--       session.system_prompt = session.system_prompt .. "\n\nCite ticket IDs."
--     end
--   end)

-- This list is the shipped set. This is a fragment: the operator's entry
-- for the same name wins, so `{ "reflection", enabled = false }` in init.lua
-- turns one off. A name here is a directory under runtime/plugins/. A
-- `require` here would run the plugin now, before the operator's init.lua
-- can disable it.
cru.plugin.setup({
  "auto-title", "consolidation", "crucible-help", "daily-notes", "discord",
  "graph", "kanban", "oci", "reflection", "retrieval-lab", "review",
  "session-board", "todo-list", "web-search", "worktree",
})
