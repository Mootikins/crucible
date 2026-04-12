-- Unit tests for path remapping logic
-- Run with: cru plugin test runtime/plugins/oci

-- Extract remap_path from init.lua by loading it directly
-- Since remap_path is local, we test it via a standalone copy

local function remap_path(workspace_host, path)
  if not path then return "/workspace" end
  if path:sub(1, #workspace_host) == workspace_host then
    local suffix = path:sub(#workspace_host + 1)
    if suffix == "" or suffix == "/" then return "/workspace" end
    if suffix:sub(1, 1) == "/" then suffix = suffix:sub(2) end
    return "/workspace/" .. suffix
  elseif path:sub(1, 1) == "/" then
    return path
  else
    return "/workspace/" .. path
  end
end

describe("remap_path", function()
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

local function sq(s)
  return s:gsub("'", "'\\''")
end

describe("sq (shell quote)", function()
  it("escapes single quotes", function()
    assert.equals("it'\\''s", sq("it's"))
  end)

  it("leaves clean strings unchanged", function()
    assert.equals("hello", sq("hello"))
  end)
end)
