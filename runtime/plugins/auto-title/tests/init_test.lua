-- Integration tests for the auto-title plugin entry point.
-- Run with: cru plugin test runtime/plugins/auto-title
--
-- Loads the REAL init.lua against a scripted `cru.sessions.complete`, so these
-- fail if the plugin stops publishing its channel, stops declaring its
-- command, or stops sanitizing what the model answered.

local publications = {}
local completions = {}
local next_answer = { "  A perfectly good title  " }

crucible = crucible or {}
crucible.publish = function(key, value) publications[key] = value end

cru = cru or {}
cru.log = function() end
cru.sessions = cru.sessions or {}
cru.sessions.complete = function(session_id, opts)
  table.insert(completions, { session_id = session_id, opts = opts })
  return next_answer[1], next_answer[2]
end

local plugin

--- Reload the plugin, so each test starts from the shipped defaults.
---
--- The reload is the reset. The plugin's config is an upvalue of its own
--- chunk, so re-requiring a cached module would hand back the table a previous
--- test had already configured — `clip = 4` set by one case would clip every
--- later case's prompt, and which cases passed would depend on the order they
--- ran in. Dropping the `package.loaded` entry first is what makes the require
--- execute the file again.
local function fresh()
  publications = {}
  completions = {}
  next_answer = { "  A perfectly good title  " }
  -- Loaded by plugin name via the parent's `?/init.lua` path entry — the same
  -- resolution the daemon's plugin loader performs.
  package.loaded["auto-title"] = nil
  plugin = require("auto-title")
end

describe("declaration", function()
  it("publishes the session_title channel with its command", function()
    fresh()
    assert.truthy(publications["session_title"])
    assert.equals("auto-title.generate", publications["session_title"].command)
  end)

  -- Publishing belongs to loading the plugin, not to configuring it: setup()
  -- runs again from the user's init.lua, where `crucible.publish` is bound to
  -- another plugin's name.
  it("publishes without setup() being called", function()
    fresh()
    assert.truthy(publications["session_title"])

    publications = {}
    plugin.setup({ clip = 100 })
    assert.is_nil(publications["session_title"])
  end)

  it("declares the command it published", function()
    fresh()
    assert.truthy(plugin.commands["auto-title.generate"])
    assert.equals("function", type(plugin.commands["auto-title.generate"].fn))
  end)

  -- The user's init.lua reaches this plugin by `require("auto-title")`. The
  -- daemon loads it by path instead, so without the module registering itself
  -- that require would build a second, unrelated copy.
  it("registers itself in package.loaded so require() finds this copy", function()
    fresh()
    assert.equals(plugin, package.loaded["auto-title"])
    assert.equals(plugin, require("auto-title"))
  end)
end)

describe("auto-title.generate", function()
  it("asks the session's own model and sanitizes the answer", function()
    fresh()
    next_answer = { '"Fixing the auth flow."' }

    local result = plugin.commands["auto-title.generate"].fn({
      session_id = "chat-1",
      user = "help me fix the auth flow",
      assistant = "sure, where does it break?",
    })

    assert.equals("Fixing the auth flow", result.title)
    assert.equals(1, #completions)
    assert.equals("chat-1", completions[1].session_id)
    assert.truthy(completions[1].opts.system:find("3 to 7 words", 1, true))
    assert.equals(
      "User: help me fix the auth flow\n\nAssistant: sure, where does it break?",
      completions[1].opts.prompt
    )
  end)

  -- The whole point of the port: the prompt is data, not compiled code.
  it("uses a configured prompt, clip and timeout", function()
    fresh()
    plugin.setup({ prompt = "Name it.", clip = 4, timeout = 7 })

    plugin.commands["auto-title.generate"].fn({ session_id = "chat-2", user = "abcdefgh" })

    assert.equals("Name it.", completions[1].opts.system)
    assert.equals("User: abcd", completions[1].opts.prompt)
    assert.equals(7, completions[1].opts.timeout)
  end)

  -- Merged, not replaced: setting one key from init.lua must not drop the
  -- others the TOML section set.
  it("merges successive setup calls", function()
    fresh()
    plugin.setup({ clip = 4 })
    plugin.setup({ timeout = 9 })

    plugin.commands["auto-title.generate"].fn({ session_id = "chat-3", user = "abcdefgh" })

    assert.equals("User: abcd", completions[1].opts.prompt)
    assert.equals(9, completions[1].opts.timeout)
  end)

  -- Order-independence, asserted rather than hoped for: a case that configures
  -- the plugin must not decide what the next case sees.
  it("starts from the shipped defaults after a reload", function()
    fresh()
    plugin.setup({ prompt = "Name it.", clip = 4, timeout = 7 })

    fresh()
    plugin.commands["auto-title.generate"].fn({ session_id = "chat-6", user = "abcdefgh" })

    assert.truthy(completions[1].opts.system:find("3 to 7 words", 1, true))
    assert.equals("User: abcdefgh", completions[1].opts.prompt)
    assert.is_nil(completions[1].opts.timeout)
  end)

  -- Raising is what puts the daemon on its truncation fallback. Answering with
  -- a second-best title here would make which fallback you got depend on where
  -- the failure happened.
  it("raises when the completion fails", function()
    fresh()
    next_answer = { nil, "no API key" }
    assert.has_error(function()
      plugin.commands["auto-title.generate"].fn({ session_id = "chat-4", user = "hi" })
    end)
  end)

  it("raises when there is no user message", function()
    fresh()
    assert.has_error(function()
      plugin.commands["auto-title.generate"].fn({ session_id = "chat-5" })
    end)
    assert.equals(0, #completions)
  end)
end)
