-- Crucible built-in Lua defaults
-- Loaded into every daemon session VM automatically.
-- Override by creating .crucible/lua/init.lua in your project directory.

-- Precognition context formatter
--
-- Customizes how relevant notes are injected before each LLM turn.
-- Receives: ctx, event — one flat table, payload fields at the top level
-- (event.user_message, event.results, ...). Return a string to use as the
-- context block, or nil to fall back.
--
-- Override example in your .crucible/lua/init.lua:
--   cru.on("precognition_format", function(ctx, event)
--     return "## Notes\n" .. event.user_message
--   end)
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
--   permissions "ask" | "allow" | "deny"
--
-- These are declarations, not enforcement. The daemon independently filters
-- plan mode's advertised tool set and refuses plugin tools at dispatch, and
-- the things that must not be widenable — [permissions] deny rules, workspace
-- containment, the shell policy — are outside modes entirely and stay
-- unconditional. Widening a mode here cannot widen those.
--
-- Override or extend from your own init.lua, which runs after this file:
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
cru.modes.normal = {
  description = "Full read/write access",
  tools = "*",
  permissions = "ask",
}

-- Plan's stance is "ask", not "deny", because its real rule is CONDITIONAL
-- and a stance can only be static: plan forbids mutation, not reading, and an
-- agent card's `ask` policy can push a read-only tool through the gate. The
-- hook below expresses that. This is the split the two tiers exist for.
cru.modes.plan = {
  description = "Read-only exploration",
  tools = {
    "read_*", "list_*", "get_*", "*_search", "glob", "grep", "skill_view",
  },
  permissions = "ask",
}

cru.modes.auto = {
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
-- Registered at priority 1000 so your own hooks are asked first.
cru.permissions.on_request(function(request)
  if request.mode == "plan" and not request.is_safe then
    return { deny = true }
  end
  return nil
end, { priority = 1000 })

-- Default system prompt
--
-- `cru.defaults.x` is the value a NEW session starts from; `session.x` is
-- that session's own value. (The pair Neovim spells `vim.o` / `vim.bo`. It is
-- not called `crucible.o` because `vim.o` doubles as an alias for the current
-- buffer, and the daemon multiplexes sessions — there is no current one.)
--
-- An agent card's own system_prompt wins: defaults only fill fields that were
-- left unset.
--
-- This is an ordinary assignment, so overriding it is ordinary Lua. In your
-- ~/.config/crucible/init.lua, which runs after this file:
--
--   cru.defaults.system_prompt = "…"                        -- replace
--   cru.defaults.system_prompt =                            -- append
--     cru.defaults.system_prompt .. "\n\nAnswer in British English."
--
-- and per session, for anything conditional:
--
--   cru.on_session_start(function(session)
--     if session.workspace:match("/work/") then
--       session.system_prompt = session.system_prompt .. "\n\nCite ticket IDs."
--     end
--   end)
cru.defaults.system_prompt = table.concat({
  "You are Crucible, a knowledge-grounded agent working alongside the user.",
  "",
  "Ground your answers in the notes and context you are given. When context",
  "is missing, say so and offer to look — never invent a note, a path, or a",
  "quotation. Reference notes by title, and link them with [[wikilinks]] when",
  "you write to the kiln.",
  "",
  "Use your tools rather than guessing: read a file before describing it, and",
  "verify a change before reporting it done. Prefer one decisive action over a",
  "list of options.",
  "",
  "Be concise. Match the depth of the question — a short question gets a short",
  "answer, and code or structure only when it earns its place.",
}, "\n")

-- Bundled plugin defaults
--
-- These load runtime plugins with default config. Override in your
-- .crucible/lua/init.lua by calling setup() with custom config:
--
--   require("kiln-expert").setup({
--     kilns = { docs = "~/crucible/docs" },
--     timeout = 60,
--   })
--
-- Or don't require a plugin at all to skip loading it.
pcall(function()
  local ke = require("kiln-expert")
  if ke and ke.setup then ke.setup({}) end
end)
