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

--- Stub `cru.shell.exec` and hand the recorded call to `fn`.
---
--- Saved and restored like `with_path`: the plugin test files share one Lua VM,
--- so a stub left installed leaks into whatever runs next.
local function with_exec(fn)
  local calls = {}
  local saved = cru.shell
  cru.shell = {
    exec = function(cmd, args, opts)
      table.insert(calls, { cmd = cmd, args = args, opts = opts })
      return { success = true, exit_code = 0, stdout = "", stderr = "" }
    end,
    -- Recorded the same way, so a test can assert which of the two a caller
    -- chose — `build` streams only when given an `on_progress`.
    spawn = function(cmd, args, opts)
      table.insert(calls, { cmd = cmd, args = args, opts = opts, streamed = true })
      return { success = true, exit_code = 0, stdout = "", stderr = "" }
    end,
    which = saved and saved.which,
  }
  local ok, err = pcall(fn, calls)
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

  it("mounts the workspace at /workspace when no target is resolved", function()
    local args = base_args()
    local i = index_of(args, "/home/user/project:/workspace:rw,z")
    assert.is_not_nil(i)
    assert.equals("-v", args[i - 1])
    local w = index_of(args, "-w")
    assert.equals("/workspace", args[w + 1])
  end)

  -- A devcontainer's workspaceFolder is typically /workspaces/<name>. The bind
  -- target and the working directory have to move together: mounting at one
  -- path and starting in another puts every relative tool call in an empty
  -- directory.
  it("mounts and works in the resolved target", function()
    local args = base_args({ target = "/workspaces/project" })
    local i = index_of(args, "/home/user/project:/workspaces/project:rw,z")
    assert.is_not_nil(i, "the bind mount must follow the resolved target")
    assert.equals("-v", args[i - 1])

    local w = index_of(args, "-w")
    assert.equals("/workspaces/project", args[w + 1])
    assert.is_nil(index_of(args, "/workspace"), "no /workspace left behind")
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

  -- A devcontainer's `runArgs` is raw runtime argv. It has to land before the
  -- image, or it becomes arguments to the sidecar command instead of flags.
  it("splices run_args in before the image", function()
    local args = base_args({ run_args = { "--cap-add", "SYS_PTRACE" } })
    local i = index_of(args, "--cap-add")
    assert.is_not_nil(i)
    assert.equals("SYS_PTRACE", args[i + 1])
    assert.truthy(i < index_of(args, "alpine:latest"), "run_args must precede the image")
  end)

  -- A devcontainer's `remoteUser`. Distinct from the uid-mapping pin, which
  -- names a numeric host id; this names a user in the image.
  it("runs as a resolved user", function()
    local args = base_args({ user = "vscode" })
    local i = index_of(args, "--user")
    assert.is_not_nil(i)
    assert.equals("vscode", args[i + 1])
  end)

  -- The measured pairing wins when both are present: keep-id maps the *host*
  -- uid into the container, and a --user naming anything else fails every
  -- workspace write. See container.lua for the measurements.
  it("prefers the mapped host uid over a resolved user", function()
    local args = base_args({ user = "vscode", userns = "keep-id", run_as_uid = "1000", run_as_gid = "1000" })
    local i = index_of(args, "--user")
    assert.equals("1000:1000", args[i + 1])
    assert.is_nil(index_of(args, "vscode"))
  end)
end)

-- `build_timeout` / `start_timeout` are documented options. They were dropped
-- on the way through, so 900/300 were the only values anyone could ever get and
-- a cold pull on a slow link failed at a limit the config said was raised.
describe("container timeouts", function()
  local run_opts = {
    name = "crucible-s1",
    session_id = "s1",
    workspace = "/home/user/project",
    image = "alpine:latest",
  }

  local function with_extra(base, overrides)
    local opts = {}
    for k, v in pairs(base) do opts[k] = v end
    for k, v in pairs(overrides) do opts[k] = v end
    return opts
  end

  it("defaults the start timeout to 300 seconds", function()
    with_exec(function(calls)
      container.run("podman", run_opts)
      assert.equals(300, calls[1].opts.timeout)
    end)
  end)

  it("honours a configured start timeout", function()
    with_exec(function(calls)
      container.run("podman", with_extra(run_opts, { start_timeout = 60 }))
      assert.equals(60, calls[1].opts.timeout)
    end)
  end)

  it("defaults the build timeout to 900 seconds", function()
    with_exec(function(calls)
      container.build("podman", {
        image = "alpine:latest", dockerfile = "Dockerfile", context = "/home/user/project",
      })
      assert.equals(900, calls[1].opts.timeout)
    end)
  end)

  it("honours a configured build timeout", function()
    with_exec(function(calls)
      container.build("podman", {
        image = "alpine:latest", dockerfile = "Dockerfile", context = "/home/user/project",
        build_timeout = 1800,
      })
      assert.equals(1800, calls[1].opts.timeout)
    end)
  end)
end)

-- A devcontainer's `build.args`. Absent, the image is built with the
-- Dockerfile's own ARG defaults — a different image than the editor's, quietly.
describe("container.build args", function()
  it("passes each build arg as --build-arg", function()
    with_exec(function(calls)
      container.build("podman", {
        image = "dc:latest", dockerfile = "/ws/.devcontainer/Dockerfile",
        context = "/ws/.devcontainer", build_args = { VARIANT = "1.83" },
      })
      local args = calls[1].args
      local i = index_of(args, "--build-arg")
      assert.is_not_nil(i)
      assert.equals("VARIANT=1.83", args[i + 1])
      assert.equals("/ws/.devcontainer", args[#args], "the context stays last")
    end)
  end)
end)
