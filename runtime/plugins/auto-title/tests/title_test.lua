-- Unit tests for the auto-title plugin's pure title logic.
-- Run with: cru plugin test runtime/plugins/auto-title
--
-- The four sanitize cases are ported from the Rust tests that shipped with
-- `provider/title.rs`; each one names a thing a model actually did.

local title = require("auto_title")

describe("sanitize", function()
  it("strips wrapping quotes and a trailing period", function()
    assert.equals("Fixing the auth flow", title.sanitize('"Fixing the auth flow."'))
  end)

  it("strips Title: scaffolding and takes the first line", function()
    assert.equals(
      "Session archiving sweep",
      title.sanitize("Title: Session archiving sweep\n\nExplanation follows")
    )
  end)

  it("collapses whitespace and caps the length", function()
    local long = string.rep("word ", 40)
    local result = title.sanitize(long)
    assert.truthy(utf8.len(result) <= 80)
    assert.equals("...", result:sub(-3))
  end)

  it("yields an empty title for empty input", function()
    assert.equals("", title.sanitize("   \n  "))
    assert.equals("", title.sanitize(nil))
  end)

  it("strips curly quotes and backticks too", function()
    assert.equals("Refactor the loader", title.sanitize("\u{201c}Refactor the loader\u{201d}"))
    assert.equals("Refactor the loader", title.sanitize("`Refactor the loader`"))
  end)

  -- A cap that counted bytes would cut a multi-byte character in half and
  -- produce a title no client can render.
  it("caps by codepoint, not by byte", function()
    local result = title.sanitize(string.rep("日", 200))
    assert.equals(80, utf8.len(result))
  end)
end)

describe("clip", function()
  it("passes short text through untouched", function()
    assert.equals("hello", title.clip("hello", 1500))
  end)

  it("keeps whole characters when it cuts", function()
    local result = title.clip(string.rep("日", 10), 4)
    assert.equals(4, utf8.len(result))
    assert.equals("日日日日", result)
  end)

  it("answers with an empty string for a missing message", function()
    assert.equals("", title.clip(nil, 10))
  end)
end)

describe("exchange", function()
  it("labels both turns", function()
    assert.equals("User: hi\n\nAssistant: hello", title.exchange("hi", "hello"))
  end)

  -- An empty `Assistant:` line is something the model tries to account for.
  it("leaves out an absent or empty assistant turn", function()
    assert.equals("User: hi", title.exchange("hi", nil))
    assert.equals("User: hi", title.exchange("hi", ""))
  end)

  it("clips each turn independently", function()
    local long = string.rep("a", 100)
    local result = title.exchange(long, long, 10)
    assert.equals("User: aaaaaaaaaa\n\nAssistant: aaaaaaaaaa", result)
  end)
end)
