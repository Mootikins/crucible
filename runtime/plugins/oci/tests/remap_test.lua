--!strict
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
    expect.equals(
      "/workspace/src/main.rs",
      remap_path("/home/user/project", "/home/user/project/src/main.rs", "/workspace")
    )
  end)

  it("remaps relative path under the mount target", function()
    expect.equals(
      "/workspace/src/main.rs",
      remap_path("/home/user/project", "src/main.rs", "/workspace")
    )
  end)

  it("passes through absolute paths outside workspace", function()
    expect.equals(
      "/etc/passwd",
      remap_path("/home/user/project", "/etc/passwd", "/workspace")
    )
  end)

  it("remaps workspace root itself", function()
    expect.equals(
      "/workspace",
      remap_path("/home/user/project", "/home/user/project", "/workspace")
    )
  end)

  it("remaps workspace root with trailing slash", function()
    expect.equals(
      "/workspace",
      remap_path("/home/user/project", "/home/user/project/", "/workspace")
    )
  end)

  it("returns the mount target for nil path", function()
    expect.equals(
      "/workspace",
      remap_path("/home/user/project", nil, "/workspace")
    )
  end)

  it("handles nested subdirectories", function()
    expect.equals(
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
    expect.equals(
      "/workspaces/project/src/main.rs",
      remap_path(ws, ws .. "/src/main.rs", target)
    )
  end)

  it("remaps a relative path under the resolved target", function()
    expect.equals(
      "/workspaces/project/src/main.rs",
      remap_path(ws, "src/main.rs", target)
    )
  end)

  it("remaps the workspace root to the resolved target", function()
    expect.equals(target, remap_path(ws, ws, target))
    expect.equals(target, remap_path(ws, ws .. "/", target))
  end)

  it("returns the resolved target for nil path", function()
    expect.equals(target, remap_path(ws, nil, target))
  end)

  it("still passes through paths outside the workspace untouched", function()
    expect.equals("/etc/passwd", remap_path(ws, "/etc/passwd", target))
  end)
end)

-- One default, shared with the mount container.lua creates — the two must
-- agree or every remapped path names a directory the container does not have.
describe("remap.DEFAULT_TARGET", function()
  it("is /workspace", function()
    expect.equals("/workspace", remap.DEFAULT_TARGET)
  end)

  it("is what remap_path uses when no target is resolved", function()
    expect.equals(
      "/workspace/src/main.rs",
      remap_path("/home/user/project", "src/main.rs")
    )
  end)
end)

describe("remap.sq (shell quote)", function()
  it("escapes single quotes", function()
    expect.equals("it'\\''s", sq("it's"))
  end)

  it("leaves clean strings unchanged", function()
    expect.equals("hello", sq("hello"))
  end)
end)

describe("remap.truncate_lines", function()
  it("keeps everything under the limit with a count footer", function()
    local out = truncate_lines({ "a", "b" }, 10, "files")
    expect.equals("a\nb\n\n[2 files]", out)
  end)

  it("truncates over the limit and says so", function()
    local out = truncate_lines({ "a", "b", "c" }, 2, "matches")
    expect.equals("a\nb\n\n[2 matches, truncated at 2]", out)
  end)
end)

-- The gap that let a containment regression through review: every case here
-- passed a real workspace root, so nothing covered the one input where the
-- prefix test is degenerate.
describe("remap_path with no workspace root", function()
  it("reparents an absolute path rather than passing it through", function()
    -- `path:sub(1, 0)` is `""`, so an empty root matches everything and the
    -- strip branch runs. Reparenting is the containing answer: passing the
    -- path through would name a HOST file from inside the container.
    expect.equals("/workspace/etc/passwd", remap.remap_path("", "/etc/passwd", "/workspace"))
  end)

  it("treats a nil root the same way", function()
    expect.equals("/workspace/etc/passwd", remap.remap_path(nil, "/etc/passwd", "/workspace"))
  end)

  it("still answers the target itself for the root path", function()
    expect.equals("/workspace", remap.remap_path("", "/", "/workspace"))
  end)
end)
