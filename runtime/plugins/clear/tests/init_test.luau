--!strict
-- The shipped slash command passes the active session to the public clear primitive.
local calls: { { session_id: string } } = {}
;(cru :: any).builtin_clear = function(args: { session_id: string })
  table.insert(calls, args)
  return true
end

local plugin = require("clear")

describe("clear command", function()
  it("passes the active session to the default implementation", function()
    local args = { session_id = "chat-1" }
    expect.equals(true, plugin.commands.clear.fn(args))
    expect.equals(1, #calls)
    expect.equals("chat-1", calls[1].session_id)
  end)
end)
