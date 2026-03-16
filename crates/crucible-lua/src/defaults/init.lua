-- Crucible built-in Lua defaults
-- Loaded into every daemon session VM automatically.
-- Override by creating .crucible/lua/init.lua in your project directory.

-- Precognition context formatter
--
-- Customizes how relevant notes are injected before each LLM turn.
-- Receives: ctx, event (or event.payload depending on runtime shape).
-- Return a string to use as the context block, or nil to fall back.
--
-- Override example in your .crucible/lua/init.lua:
--   crucible.on("precognition_format", function(ctx, event)
--     local payload = event.payload or event
--     return "## Notes\n" .. payload.user_message
--   end)
if crucible and type(crucible.on) == "function" then
  crucible.on("precognition_format", function(ctx, event)
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
end

-- Session start hook
--
-- Fires when a new session begins. Useful for setting per-session defaults.
--
-- Override example in your .crucible/lua/init.lua:
--   crucible.on_session_start(function(session)
--     session.temperature = 0.3
--   end)
if crucible and type(crucible.on_session_start) == "function" then
  crucible.on_session_start(function(session)
    -- Default no-op. Override to set session fields as needed.
  end)
end
