--!strict
local requested: string? = nil
cru.session.generate_title = function(id)
  requested = id
  return "Canonical title", nil
end
local plugin = require("auto-title")
describe("generate compatibility command", function()
  it("forwards the session to the daemon owner", function()
    local result = plugin.commands.generate.fn({}, { session_id = "s1" })
    expect.equals("s1", requested)
    expect.equals("Session titled: Canonical title", result)
  end)
  it("requires a session", function()
    expect.has_error(function() plugin.commands.generate.fn({}, {}) end, "run /generate in a chat session")
  end)
end)
