-- Unit tests for path remapping and shell quoting.
-- Run with: cru plugin test runtime/plugins/oci
--
-- Requires the real module. The previous version of this file declared its
-- own copies of remap_path and sq, so it passed no matter how broken the
-- plugin was — a green badge over dead code.

local remap = require("remap")
local remap_path = remap.remap_path
local sq = remap.sq
local truncate_lines = remap.truncate_lines

describe("remap.remap_path", function()
  it("remaps absolute host path to container path", function()
    assert.equals(
      "/workspace/src/main.rs",
      remap_path("/home/user/project", "/home/user/project/src/main.rs")
    )
  end)

  it("remaps relative path under /workspace", function()
    assert.equals(
      "/workspace/src/main.rs",
      remap_path("/home/user/project", "src/main.rs")
    )
  end)

  it("passes through absolute paths outside workspace", function()
    assert.equals(
      "/etc/passwd",
      remap_path("/home/user/project", "/etc/passwd")
    )
  end)

  it("remaps workspace root itself", function()
    assert.equals(
      "/workspace",
      remap_path("/home/user/project", "/home/user/project")
    )
  end)

  it("remaps workspace root with trailing slash", function()
    assert.equals(
      "/workspace",
      remap_path("/home/user/project", "/home/user/project/")
    )
  end)

  it("returns /workspace for nil path", function()
    assert.equals(
      "/workspace",
      remap_path("/home/user/project", nil)
    )
  end)

  it("handles nested subdirectories", function()
    assert.equals(
      "/workspace/a/b/c/deep.txt",
      remap_path("/home/user/project", "/home/user/project/a/b/c/deep.txt")
    )
  end)
end)

describe("remap.sq (shell quote)", function()
  it("escapes single quotes", function()
    assert.equals("it'\\''s", sq("it's"))
  end)

  it("leaves clean strings unchanged", function()
    assert.equals("hello", sq("hello"))
  end)
end)

describe("remap.truncate_lines", function()
  it("keeps everything under the limit with a count footer", function()
    local out = truncate_lines({ "a", "b" }, 10, "files")
    assert.equals("a\nb\n\n[2 files]", out)
  end)

  it("truncates over the limit and says so", function()
    local out = truncate_lines({ "a", "b", "c" }, 2, "matches")
    assert.equals("a\nb\n\n[2 matches, truncated at 2]", out)
  end)
end)
