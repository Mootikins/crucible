-- Unit tests for container runtime resolution and the `run` argv.
-- Run with: cru plugin test runtime/plugins/oci

local container = require("container")

--- Stub `cru.shell.which` so runtime probing is deterministic.
local function with_path(available, fn)
  local present = {}
  for _, name in ipairs(available) do present[name] = true end

  local saved = cru.shell
  cru.shell = {
    which = function(cmd)
      return present[cmd] and ("/usr/bin/" .. cmd) or nil
    end,
  }
  local ok, err = pcall(fn)
  cru.shell = saved
  if not ok then error(err, 0) end
end

--- Index of `needle` in an argv list, or nil.
local function index_of(args, needle)
  for i, v in ipairs(args) do
    if v == needle then return i end
  end
  return nil
end

describe("container.detect", function()
  it("prefers podman when nothing is configured", function()
    with_path({ "podman", "docker", "nerdctl" }, function()
      assert.equals("podman", container.detect(nil))
    end)
  end)

  it("falls through to docker when podman is absent", function()
    with_path({ "docker", "nerdctl" }, function()
      assert.equals("docker", container.detect(nil))
    end)
  end)

  it("falls through to nerdctl when neither podman nor docker is present", function()
    with_path({ "nerdctl" }, function()
      assert.equals("nerdctl", container.detect(nil))
    end)
  end)

  it("honours a configured runtime over the probe order", function()
    with_path({ "podman", "docker" }, function()
      assert.equals("docker", container.detect("docker"))
    end)
  end)

  it("reports a configured runtime that is not installed rather than substituting one", function()
    with_path({ "podman" }, function()
      local runtime, err = container.detect("docker")
      assert.is_nil(runtime)
      assert.truthy(err:find("docker", 1, true))
    end)
  end)

  it("reports when no runtime is available at all", function()
    with_path({}, function()
      local runtime, err = container.detect(nil)
      assert.is_nil(runtime)
      assert.truthy(err:find("no container runtime", 1, true))
    end)
  end)
end)

describe("container.run_args", function()
  local function base_args(overrides)
    local opts = {
      name = "crucible-s1",
      session_id = "s1",
      workspace = "/home/user/project",
      image = "alpine:latest",
    }
    for k, v in pairs(overrides or {}) do opts[k] = v end
    return container.run_args(opts)
  end

  it("mounts the workspace at /workspace", function()
    local args = base_args()
    local i = index_of(args, "/home/user/project:/workspace:rw,z")
    assert.is_not_nil(i)
    assert.equals("-v", args[i - 1])
  end)

  it("labels the container with its session so orphans are identifiable", function()
    local args = base_args()
    assert.is_not_nil(index_of(args, "crucible.session=s1"))
    assert.is_not_nil(index_of(args, "crucible=true"))
  end)

  it("keeps no-new-privileges on", function()
    local args = base_args()
    assert.is_not_nil(index_of(args, "no-new-privileges"))
  end)

  it("omits --userns when none is resolved", function()
    local args = base_args()
    for _, v in ipairs(args) do
      assert.falsy(v:find("^%-%-userns"))
    end
  end)

  it("passes a resolved userns through", function()
    local args = base_args({ userns = "keep-id" })
    assert.is_not_nil(index_of(args, "--userns=keep-id"))
  end)

  it("ends with the image and the sidecar command", function()
    local args = base_args()
    assert.equals("infinity", args[#args])
    assert.equals("sleep", args[#args - 1])
    assert.equals("alpine:latest", args[#args - 2])
  end)

  it("appends extra mounts and env", function()
    local args = base_args({
      mounts = { "/cache:/cache:ro" },
      env = { FOO = "bar" },
    })
    assert.is_not_nil(index_of(args, "/cache:/cache:ro"))
    assert.is_not_nil(index_of(args, "FOO=bar"))
  end)
end)
