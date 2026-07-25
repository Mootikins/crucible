-- Integration tests for the oci plugin entry point.
-- Run with: cru plugin test runtime/plugins/oci
--
-- Loads the REAL init.lua. Everything the plugin touches at load and at
-- session lifecycle is stubbed with recorders installed before the require,
-- so these tests fail if init.lua stops loading, stops registering handlers,
-- or stops claiming isolation — the three ways it has actually broken.

-- ─────────────────────────────────────────────────────────────────────────────
-- Recorders, installed before require("init") because init.lua registers its
-- tool handlers at load time.
-- ─────────────────────────────────────────────────────────────────────────────

local hooks = {}          -- pattern -> { fn, opts }
local lifecycle = {}      -- start = {fn, opts}, end_fn
local isolation_calls = {}
local status_calls = {}
local clear_status_calls = {}

crucible = crucible or {}
crucible.on = function(event, opts, fn)
  assert.equals("pre_tool_call", event)
  hooks[opts.pattern] = { fn = fn, opts = opts }
end
crucible.on_session_start = function(fn, opts)
  lifecycle.start = { fn = fn, opts = opts or {} }
end
crucible.on_session_end = function(fn)
  lifecycle.end_fn = fn
end
crucible.require_isolation = function(opts)
  table.insert(isolation_calls, opts)
end
crucible.set_status = function(opts)
  table.insert(status_calls, opts)
end
crucible.clear_status = function(opts)
  table.insert(clear_status_calls, opts)
end

-- Scripted shell. Responders are keyed by "<cmd> <first-arg>" (then "<cmd>"),
-- and every call is logged so tests can assert on the exact argv.
local exec_log = {}
local responders = {}
local ok_reply = { success = true, exit_code = 0, stdout = "", stderr = "" }

local function stub_exec(cmd, args, opts)
  table.insert(exec_log, { cmd = cmd, args = args, opts = opts })
  local r = responders[cmd .. " " .. (args[1] or "")] or responders[cmd]
  if type(r) == "function" then return r(cmd, args, opts) end
  return r or ok_reply
end

local available = { podman = true }
local function stub_which(name)
  return available[name] and ("/usr/bin/" .. name) or nil
end

local function install_shell()
  cru.shell = { exec = stub_exec, which = stub_which }
  cru.json = { encode = function(t) return t end }
  cru.log = function() end
end

cru = cru or {}
install_shell()

-- Loaded by plugin name via the parent's `?/init.lua` path entry — the same
-- resolution the daemon's plugin loader performs, so a require that only
-- works in tests cannot slip through again.
local spec = require("oci")

--- The logged call whose argv starts with `verb`, or nil.
local function exec_call(verb)
  for _, call in ipairs(exec_log) do
    if call.args[1] == verb then return call end
  end
  return nil
end

local function index_of(args, needle)
  for i, v in ipairs(args) do
    if v == needle then return i end
  end
  return nil
end

--- Run the captured session-start hook for a fresh session.
local function start_session(id, workspace)
  return lifecycle.start.fn({ id = id, workspace = workspace or "/home/user/project" })
end

describe("oci plugin load", function()
  it("returns its spec", function()
    assert.equals("oci", spec.name)
    assert.is_function(spec.setup)
  end)

  it("registers every workspace tool handler at load, not per session", function()
    for _, tool in ipairs({ "bash", "read_file", "write_file", "edit_file", "glob", "grep" }) do
      assert.is_not_nil(hooks[tool], "no pre_tool_call handler for " .. tool)
      assert.equals(10, hooks[tool].opts.priority)
    end
  end)

  it("marks its session-start hook required so a broken sandbox refuses the session", function()
    assert.is_not_nil(lifecycle.start)
    assert.truthy(lifecycle.start.opts.required)
    assert.is_not_nil(lifecycle.end_fn)
  end)
end)

describe("oci session lifecycle", function()
  before_each(function()
    install_shell()
    exec_log = {}
    responders = {}
    isolation_calls = {}
    status_calls = {}
    clear_status_calls = {}
    available = { podman = true }
  end)

  it("does nothing when no image is configured", function()
    spec.setup(nil)
    start_session("s-unconfigured")
    assert.equals(0, #exec_log, "an unconfigured plugin must not touch the runtime")
    assert.equals(0, #isolation_calls)
  end)

  it("starts a container, claims isolation, and publishes status", function()
    spec.setup({ image = "alpine:latest", exempt = { "read_note" } })
    start_session("s1")

    local run = exec_call("run")
    assert.is_not_nil(run, "expected a container run")
    assert.equals("podman", run.cmd)
    assert.is_not_nil(index_of(run.args, "crucible-s1"))
    assert.is_not_nil(index_of(run.args, "/home/user/project:/workspace:rw,z"))

    assert.equals(1, #isolation_calls)
    assert.equals("s1", isolation_calls[1].session)
    assert.equals("oci", isolation_calls[1].plugin)
    assert.equals("read_note", isolation_calls[1].exempt[1])

    local last = status_calls[#status_calls]
    assert.is_not_nil(last)
    assert.equals("oci", last.key)
    assert.truthy(last.text:find("sandboxed", 1, true))
    assert.truthy(last.text:find("alpine:latest", 1, true))
  end)

  it("maps the host uid when the image runs as non-root", function()
    spec.setup({ image = "dev:latest" })
    responders["podman image"] = { success = true, exit_code = 0, stdout = "appuser\n", stderr = "" }
    responders["id -u"] = { success = true, exit_code = 0, stdout = "1000\n", stderr = "" }
    responders["id -g"] = { success = true, exit_code = 0, stdout = "1000\n", stderr = "" }
    start_session("s-nonroot")

    local run = exec_call("run")
    assert.is_not_nil(index_of(run.args, "--userns=keep-id"))
    local user_flag = index_of(run.args, "--user")
    assert.is_not_nil(user_flag, "keep-id without --user does not fix workspace writes")
    assert.equals("1000:1000", run.args[user_flag + 1])
  end)

  it("leaves uid mapping off for a root image", function()
    spec.setup({ image = "alpine:latest" })
    responders["podman image"] = { success = true, exit_code = 0, stdout = "\n", stderr = "" }
    start_session("s-root")

    local run = exec_call("run")
    for _, v in ipairs(run.args) do
      assert.falsy(v:find("^%-%-userns"))
    end
  end)

  it("raises when the container fails to start, refusing the session", function()
    spec.setup({ image = "alpine:latest" })
    responders["podman run"] = { success = false, exit_code = 125, stdout = "", stderr = "boom" }
    local ok, err = pcall(start_session, "s-fail")
    assert.falsy(ok, "a session whose sandbox did not start must be refused")
    assert.truthy(tostring(err):find("container start failed", 1, true))
    assert.equals(0, #isolation_calls, "no isolation claim for a container that never started")
  end)

  it("raises when no runtime exists rather than running unsandboxed", function()
    spec.setup({ image = "alpine:latest" })
    available = {}
    local ok, err = pcall(start_session, "s-noruntime")
    assert.falsy(ok)
    assert.truthy(tostring(err):find("no container runtime", 1, true))
  end)

  it("raises when the session has no workspace to isolate", function()
    spec.setup({ image = "alpine:latest" })
    local ok, err = pcall(lifecycle.start.fn, { id = "s-nows", workspace = "" })
    assert.falsy(ok)
    assert.truthy(tostring(err):find("no workspace", 1, true))
  end)

  it("stops and removes the container and clears status on session end", function()
    spec.setup({ image = "alpine:latest" })
    start_session("s-end")
    exec_log = {}

    lifecycle.end_fn({ id = "s-end" })
    local stop = exec_call("stop")
    local rm = exec_call("rm")
    assert.is_not_nil(stop)
    assert.is_not_nil(index_of(stop.args, "crucible-s-end"))
    assert.is_not_nil(rm)
    assert.is_not_nil(index_of(rm.args, "crucible-s-end"))
    assert.equals(1, #clear_status_calls)
    assert.equals("oci", clear_status_calls[1].key)
  end)

  it("ignores session end for a session it never started", function()
    lifecycle.end_fn({ id = "s-stranger" })
    assert.equals(0, #exec_log)
  end)
end)

describe("oci tool interception", function()
  before_each(function()
    install_shell()
    exec_log = {}
    responders = {}
    isolation_calls = {}
    status_calls = {}
    available = { podman = true }
    spec.setup({ image = "alpine:latest" })
    start_session("s-tools", "/home/user/project")
    exec_log = {}
  end)

  it("routes bash through exec in the session's container", function()
    responders["podman exec"] = { success = true, exit_code = 0, stdout = "hi\n", stderr = "" }
    local res = hooks.bash.fn({ session_id = "s-tools" }, { tool = "bash", args = { command = "echo hi" } })

    assert.truthy(res.handled)
    assert.equals("hi\n", res.result.result)

    local call = exec_call("exec")
    assert.is_not_nil(index_of(call.args, "crucible-s-tools"))
    local sh = index_of(call.args, "sh")
    assert.equals("-c", call.args[sh + 1])
    assert.equals("echo hi", call.args[sh + 2])
  end)

  it("remaps read_file paths into /workspace", function()
    responders["podman exec"] = { success = true, exit_code = 0, stdout = "     1\tline\n", stderr = "" }
    local res = hooks.read_file.fn(
      { session_id = "s-tools" },
      { tool = "read_file", args = { path = "/home/user/project/src/main.rs" } }
    )
    assert.truthy(res.handled)

    local call = exec_call("exec")
    local sh = index_of(call.args, "sh")
    local script = call.args[sh + 2]
    assert.truthy(script:find("/workspace/src/main.rs", 1, true))
    assert.falsy(script:find("/home/user/project", 1, true))
  end)

  it("does not intercept for a session with no container", function()
    local res = hooks.bash.fn({ session_id = "s-other" }, { tool = "bash", args = { command = "echo hi" } })
    assert.is_nil(res, "a handler must no-op for sessions it does not sandbox")
    assert.equals(0, #exec_log)
  end)

  it("does not intercept when the call has no session context", function()
    local res = hooks.bash.fn({}, { tool = "bash", args = { command = "echo hi" } })
    assert.is_nil(res)
    assert.equals(0, #exec_log)
  end)
end)
