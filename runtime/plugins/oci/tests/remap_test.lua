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
      remap_path("/home/user/project", "/home/user/project/src/main.rs", "/workspace")
    )
  end)

  it("remaps relative path under the mount target", function()
    assert.equals(
      "/workspace/src/main.rs",
      remap_path("/home/user/project", "src/main.rs", "/workspace")
    )
  end)

  it("passes through absolute paths outside workspace", function()
    assert.equals(
      "/etc/passwd",
      remap_path("/home/user/project", "/etc/passwd", "/workspace")
    )
  end)

  it("remaps workspace root itself", function()
    assert.equals(
      "/workspace",
      remap_path("/home/user/project", "/home/user/project", "/workspace")
    )
  end)

  it("remaps workspace root with trailing slash", function()
    assert.equals(
      "/workspace",
      remap_path("/home/user/project", "/home/user/project/", "/workspace")
    )
  end)

  it("returns the mount target for nil path", function()
    assert.equals(
      "/workspace",
      remap_path("/home/user/project", nil, "/workspace")
    )
  end)

  it("handles nested subdirectories", function()
    assert.equals(
      "/workspace/a/b/c/deep.txt",
      remap_path("/home/user/project", "/home/user/project/a/b/c/deep.txt", "/workspace")
    )
  end)
end)

-- The mount target is not always /workspace: a devcontainer's `workspaceFolder`
-- is typically /workspaces/<name>, and a path remapped against the wrong root
-- names a file that does not exist inside the container.
describe("remap.remap_path with a non-default mount target", function()
  local ws = "/home/user/project"
  local target = "/workspaces/project"

  it("remaps an absolute host path under the resolved target", function()
    assert.equals(
      "/workspaces/project/src/main.rs",
      remap_path(ws, ws .. "/src/main.rs", target)
    )
  end)

  it("remaps a relative path under the resolved target", function()
    assert.equals(
      "/workspaces/project/src/main.rs",
      remap_path(ws, "src/main.rs", target)
    )
  end)

  it("remaps the workspace root to the resolved target", function()
    assert.equals(target, remap_path(ws, ws, target))
    assert.equals(target, remap_path(ws, ws .. "/", target))
  end)

  it("returns the resolved target for nil path", function()
    assert.equals(target, remap_path(ws, nil, target))
  end)

  it("still passes through paths outside the workspace untouched", function()
    assert.equals("/etc/passwd", remap_path(ws, "/etc/passwd", target))
  end)
end)

-- One default, shared with the mount container.lua creates — the two must
-- agree or every remapped path names a directory the container does not have.
describe("remap.DEFAULT_TARGET", function()
  it("is /workspace", function()
    assert.equals("/workspace", remap.DEFAULT_TARGET)
  end)

  it("is what remap_path uses when no target is resolved", function()
    assert.equals(
      "/workspace/src/main.rs",
      remap_path("/home/user/project", "src/main.rs")
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
