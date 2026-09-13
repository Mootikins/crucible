--!strict
-- Integration tests for the auto-title plugin entry point.
-- Run with: cru plugin test runtime/plugins/auto-title
--
-- Loads the REAL init.lua against a scripted `cru.session.complete`, so these
-- fail if the plugin stops publishing its channel, stops declaring its
-- command, or stops sanitizing what the model answered.

-- The runner VM has no cru.plugin (the daemon registers it); tests stub into it.
cru.plugin = cru.plugin or mock({})
local publications: { [string]: any } = {}
-- `cru.session.complete` declares `opts` as `string | { [string]: any }`,
-- because a caller may pass a bare prompt. This plugin always passes the
-- table, and the assertions below read fields off it, so the recorder
-- narrows once here rather than at every call site.
local completions: { { session_id: string, opts: { [string]: any } } } = {}
-- `{ answer, error }`: the pair `cru.session.complete` answers with. A test
-- that scripts a failure sets the second and clears the first, so both
-- slots have to accept nil.
local next_answer: { string? } = { "  A perfectly good title  " }

cru.plugin.publish = function(key, value) publications[key] = value end

;(cru :: any).log = function() end
cru.session = cru.session or mock({})
cru.session.complete = function(session_id, opts)
  table.insert(completions, { session_id = session_id, opts = opts :: { [string]: any } })
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
    plugin.setup()
    expect.truthy(publications["session_title"])
    expect.equals("auto-title.generate", publications["session_title"].command)
  end)

  -- The module body publishes nothing: `cru plugin check` reports a
  -- top-level effect. setup() publishes once. A second setup() call, from
  -- the user's init.lua, runs where `cru.plugin.publish` can be bound to
  -- another plugin's name, so it publishes nothing again.
  it("publishes from setup(), once", function()
    fresh()
    expect.is_nil(publications["session_title"])

    plugin.setup()
    expect.truthy(publications["session_title"])

    publications = {}
    plugin.setup({ clip = 100 })
    expect.is_nil(publications["session_title"])
  end)

  it("declares the command it published", function()
    fresh()
    expect.truthy(plugin.commands["auto-title.generate"])
    expect.equals("function", type(plugin.commands["auto-title.generate"].fn))
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

    expect.equals("Fixing the auth flow", result.title)
    expect.equals(1, #completions)
    expect.equals("chat-1", completions[1].session_id)
    expect.truthy(completions[1].opts.system:find("3 to 7 words", 1, true))
    expect.equals(
      "User: help me fix the auth flow\n\nAssistant: sure, where does it break?",
      completions[1].opts.prompt
    )
  end)

  -- The whole point of the port: the prompt is data, not compiled code.
  it("uses a configured prompt, clip and timeout", function()
    fresh()
    plugin.setup({ prompt = "Name it.", clip = 4, timeout = 7 })

    plugin.commands["auto-title.generate"].fn({ session_id = "chat-2", user = "abcdefgh" })

    expect.equals("Name it.", completions[1].opts.system)
    expect.equals("User: abcd", completions[1].opts.prompt)
    expect.equals(7, completions[1].opts.timeout)
  end)

  -- Merged, not replaced: setting one key from init.lua must not drop the
  -- others the TOML section set.
  it("merges successive setup calls", function()
    fresh()
    plugin.setup({ clip = 4 })
    plugin.setup({ timeout = 9 })

    plugin.commands["auto-title.generate"].fn({ session_id = "chat-3", user = "abcdefgh" })

    expect.equals("User: abcd", completions[1].opts.prompt)
    expect.equals(9, completions[1].opts.timeout)
  end)

  -- Order-independence, asserted rather than hoped for: a case that configures
  -- the plugin must not decide what the next case sees.
  it("starts from the shipped defaults after a reload", function()
    fresh()
    plugin.setup({ prompt = "Name it.", clip = 4, timeout = 7 })

    fresh()
    plugin.commands["auto-title.generate"].fn({ session_id = "chat-6", user = "abcdefgh" })

    expect.truthy(completions[1].opts.system:find("3 to 7 words", 1, true))
    expect.equals("User: abcdefgh", completions[1].opts.prompt)
    expect.is_nil(completions[1].opts.timeout)
  end)

  -- Raising is what puts the daemon on its truncation fallback. Answering with
  -- a second-best title here would make which fallback you got depend on where
  -- the failure happened.
  it("raises when the completion fails", function()
    fresh()
    next_answer = { nil, "no API key" }
    expect.has_error(function()
      plugin.commands["auto-title.generate"].fn({ session_id = "chat-4", user = "hi" })
    end)
  end)

  it("raises when there is no user message", function()
    fresh()
    expect.has_error(function()
      plugin.commands["auto-title.generate"].fn({ session_id = "chat-5" })
    end)
    expect.equals(0, #completions)
  end)
end)
