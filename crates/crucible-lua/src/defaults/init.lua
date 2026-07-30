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

-- Auto mode approves every permission request
--
-- "auto" is documented as "Auto-approve all operations", and this hook is
-- what makes that true. It lives in Lua, not the daemon, so the meaning of a
-- mode stays configurable: the daemon supplies `request.mode` and the policy
-- is written here.
--
-- Runs after the `[permissions]` config and the stored pattern allowlist, so
-- an operator `deny` rule still wins — auto mode skips the *prompt*, it does
-- not override an explicit denial.
--
-- Override example in your .crucible/lua/init.lua (auto-allow reads always,
-- and keep prompting in auto mode):
--   cru.permissions.on_request(function(request)
--     if request.tool_name == "read_file" then return { allow = true } end
--     return nil
--   end)
cru.permissions.on_request(function(request)
  if request.mode == "auto" then
    return { allow = true }
  end
  return nil
end)

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
