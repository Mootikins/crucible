--!strict
-- Compatibility for the existing /generate slash command. Automatic titling
-- is core runtime behavior and does not depend on this plugin being enabled.
return {
  commands = {
    generate = {
      desc = "Regenerate the session title",
      fn = function(args, ctx)
        local session_id = (args and args.session_id) or (ctx and ctx.session_id)
        if type(session_id) ~= "string" or session_id == "" then
          error("auto-title: no session to title; run /generate in a chat session", 0)
        end
        local title, err = cru.session.generate_title(session_id)
        if not title then error("auto-title: " .. tostring(err), 0) end
        return "Session titled: " .. title
      end,
    },
  },
}
