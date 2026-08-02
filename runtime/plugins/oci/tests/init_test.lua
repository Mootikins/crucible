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
local publications = {}      -- key -> published value
local declared_options       -- the settings tree the plugin declared

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
crucible.publish = function(key, value)
  publications[key] = value
end
crucible.options = function(tree)
  declared_options = tree
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
  if r then return r end
  -- An unscripted `git` call means the test set up no repository, so answer as
  -- git does outside one. Falling back to `ok_reply` made `git show HEAD:…`
  -- return an empty *file*, which resolution then tried to parse as JSON.
  if cmd == "git" then
    return { success = false, exit_code = 128, stdout = "",
             stderr = "fatal: not a git repository" }
  end
  return ok_reply
end

local available = { podman = true }
local function stub_which(name)
  return available[name] and ("/usr/bin/" .. name) or nil
end

--- `spawn` is `exec` plus line delivery, so scripted responders drive both and
--- a test can assert on what a streaming caller reported.
local function stub_spawn(cmd, args, opts)
  local r = stub_exec(cmd, args, opts)
  if opts and opts.on_line then
    for line in (r.stdout or ""):gmatch("[^\n]+") do
      opts.on_line("stdout", line)
    end
  end
  return r
end

local function install_shell()
  cru.shell = { exec = stub_exec, spawn = stub_spawn, which = stub_which }
  cru.json = { encode = function(t) return t end }
  cru.log = function() end
end

cru = cru or {}
-- Captured before install_shell() replaces cru.json with an encode-only stub:
-- the devcontainer tests need a real decoder, and the runtime one is gone once
-- this file loads.
local real_json = cru.json
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
---
--- The default workspace is per-session, because the plugin keys containers by
--- workspace and that state lives for the daemon's lifetime — two tests sharing
--- a default would have the second join the first's container. Tests that mean
--- to share pass the same workspace explicitly.
local function start_session(id, workspace)
  return lifecycle.start.fn({ id = id, workspace = workspace or ("/home/user/" .. id) })
end

--- Same, with the per-session `isolation` param the daemon forwards untouched.
local function start_isolated_session(id, isolation, workspace)
  return lifecycle.start.fn({
    id = id,
    workspace = workspace or ("/home/user/" .. id),
    isolation = isolation,
  })
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
    start_session("s1", "/home/user/s1-project")

    local run = exec_call("run")
    assert.is_not_nil(run, "expected a container run")
    assert.equals("podman", run.cmd)
    assert.is_not_nil(index_of(run.args, "crucible-s1"))
    assert.is_not_nil(index_of(run.args, "/home/user/s1-project:/workspace:rw,z"))

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

-- One container per distinct workspace in a session tree.
--
-- A delegated child inherits its parent's workspace, so the bind mount would be
-- the same directory either way — a second container is a cold start that buys
-- nothing. Keying by workspace means the subagent case needs no config flag,
-- and when a child gets its own worktree (a distinct workspace) it gets its own
-- container for the same reason, with no branch added anywhere.
describe("oci container sharing", function()
  local function count_calls(verb)
    local n = 0
    for _, call in ipairs(exec_log) do
      if call.args[1] == verb then n = n + 1 end
    end
    return n
  end

  -- Containers are keyed by workspace and that table outlives a test, so each
  -- test gets a workspace no other test has used.
  local ws_seq = 0
  local function shared_ws()
    ws_seq = ws_seq + 1
    return "/home/user/shared" .. ws_seq
  end

  before_each(function()
    install_shell()
    exec_log = {}
    responders = {}
    isolation_calls = {}
    status_calls = {}
    clear_status_calls = {}
    available = { podman = true }
    spec.setup({ image = "alpine:latest" })
    -- The shared container is alive unless a test says otherwise.
    responders["podman inspect"] = { success = true, exit_code = 0, stdout = "true\n", stderr = "" }
  end)

  it("registers a second session against the container of the same workspace", function()
    local ws = shared_ws()
    start_session("s-parent", ws)
    exec_log = {}
    start_session("s-child", ws)

    assert.equals(0, count_calls("run"), "a shared workspace must not pay a second cold start")
    -- Claimed all the same: sharing a container is not sharing a claim, and an
    -- unclaimed session is an unsandboxed one.
    local last_claim = isolation_calls[#isolation_calls]
    assert.equals("s-child", last_claim.session)
    assert.equals("oci", last_claim.plugin)
  end)

  it("routes the second session's tools into the shared container", function()
    local ws = shared_ws()
    start_session("s-parent", ws)
    start_session("s-child", ws)
    exec_log = {}

    responders["podman exec"] = { success = true, exit_code = 0, stdout = "hi\n", stderr = "" }
    hooks.bash.fn({ session_id = "s-child" }, { tool = "bash", args = { command = "echo hi" } })
    local call = exec_call("exec")
    assert.is_not_nil(index_of(call.args, "crucible-s-parent"),
      "the child's tools must run in the container its workspace resolved to")
  end)

  it("gives a distinct workspace its own container", function()
    local ws_a, ws_b = shared_ws(), shared_ws()
    start_session("s-a", ws_a)
    exec_log = {}
    start_session("s-b", ws_b)

    assert.equals(1, count_calls("run"), "a different workspace is a different sandbox")
    local run = exec_call("run")
    assert.is_not_nil(index_of(run.args, ws_b .. ":/workspace:rw,z"))
  end)

  it("keeps a shared container alive until its last session ends", function()
    local ws = shared_ws()
    start_session("s-parent", ws)
    start_session("s-child", ws)
    exec_log = {}

    lifecycle.end_fn({ id = "s-child" })
    assert.equals(0, count_calls("rm"),
      "a parent's container must outlive its children")
    -- ...but the child's own status slot goes with it.
    assert.equals(1, #clear_status_calls)
    assert.equals("s-child", clear_status_calls[1].session)

    lifecycle.end_fn({ id = "s-parent" })
    local rm = exec_call("rm")
    assert.is_not_nil(rm)
    assert.is_not_nil(index_of(rm.args, "crucible-s-parent"))
  end)

  -- `on_session_start` fires on create, resume AND resume_from_storage, and a
  -- web history fetch calls resume_from_storage on every request. `session_end`
  -- fires once. Counting each start therefore leaked a reference per fetch and
  -- the container was never removed for the daemon's lifetime.
  it("counts a session once however many times its start hook fires", function()
    local ws = shared_ws()
    start_session("s-resumed", ws)
    start_session("s-resumed", ws)
    start_session("s-resumed", ws)
    exec_log = {}

    lifecycle.end_fn({ id = "s-resumed" })

    local rm = exec_call("rm")
    assert.is_not_nil(rm, "a re-fired start hook left the container referenced forever")
    assert.is_not_nil(index_of(rm.args, "crucible-s-resumed"))
  end)

  -- ...and the second start must not be mistaken for a second session, which
  -- would let the container go while that session was still using it.
  it("still keeps a shared container alive for a genuinely second session", function()
    local ws = shared_ws()
    start_session("s-one", ws)
    start_session("s-one", ws)
    start_session("s-two", ws)
    exec_log = {}

    lifecycle.end_fn({ id = "s-one" })
    assert.equals(0, count_calls("rm"), "s-two is still in the container")

    lifecycle.end_fn({ id = "s-two" })
    assert.is_not_nil(exec_call("rm"))
  end)

  it("refuses a session whose shared container is gone rather than running on the host", function()
    local ws = shared_ws()
    start_session("s-parent", ws)
    responders["podman inspect"] = { success = true, exit_code = 0, stdout = "false\n", stderr = "" }

    local ok, err = pcall(start_session, "s-child", ws)
    assert.falsy(ok, "a dead sandbox must refuse the session, not silently start a fresh one")
    assert.truthy(tostring(err):find("no longer running", 1, true))
  end)
end)

-- The per-session `isolation` param, forwarded by the daemon untouched as a
-- field on the Session handed to on_session_start. Resolution is first hit
-- wins: session param, then a named profile, then the bare `image` key, then
-- nothing. Everything here is a pure resolution decision, so none of it needs
-- a container runtime.
describe("oci per-session isolation", function()
  local ws_seq = 0
  local function fresh_ws()
    ws_seq = ws_seq + 1
    return "/home/user/iso" .. ws_seq
  end

  --- Assert the container was started from `image`. The argv ends with the
  --- keepalive command, so match on presence rather than position.
  local function assert_ran_image(image)
    local run = exec_call("run")
    assert.is_not_nil(run, "expected a container run")
    assert.is_not_nil(index_of(run.args, image),
      "container was not started from " .. image)
  end

  before_each(function()
    install_shell()
    exec_log = {}
    responders = {}
    isolation_calls = {}
    status_calls = {}
    clear_status_calls = {}
    available = { podman = true }
    spec.setup({
      image = "alpine:latest",
      exempt = { "read_note" },
      profiles = {
        heavy = { image = "heavy:latest" },
        other_runtime = { image = "odd:latest", runtime = "docker" },
      },
    })
  end)

  it("resolves the project's default image when the caller says nothing", function()
    start_isolated_session("iso-absent", nil, fresh_ws())
    assert_ran_image("alpine:latest")
  end)

  -- The case the opt-in exists for: a project that containerizes every session
  -- and one session that declines. Anything short of the whole resolution
  -- short-circuiting is a container the caller said no to.
  it("starts no container at all when isolation is false", function()
    start_isolated_session("iso-false", false, fresh_ws())
    assert.equals(0, #exec_log,
      "an opted-out session must not touch the runtime even though the project configures one")
    assert.equals(0, #isolation_calls,
      "no claim either: an opted-out session runs on the host, and default-deny would strand it")
  end)

  it("uses the named profile instead of the default image", function()
    start_isolated_session("iso-profile", "heavy", fresh_ws())
    assert_ran_image("heavy:latest")
  end)

  it("uses an inline isolation table instead of the default image", function()
    start_isolated_session("iso-inline", { image = "inline:latest" }, fresh_ws())
    assert_ran_image("inline:latest")
  end)

  -- Isolation asked for and not delivered is never silently downgraded — the
  -- same rule that makes a failed container start refuse the session.
  it("refuses a session naming an isolation profile that does not exist", function()
    local ok, err = pcall(start_isolated_session, "iso-unknown", "nope", fresh_ws())
    assert.falsy(ok, "an unknown profile must not fall back to the default image")
    assert.truthy(tostring(err):find("nope", 1, true))
    assert.equals(0, #exec_log)
  end)

  it("refuses an isolation table that names no image", function()
    local ok, err = pcall(start_isolated_session, "iso-imageless", { env = { A = "1" } }, fresh_ws())
    assert.falsy(ok)
    assert.truthy(tostring(err):find("no image", 1, true))
  end)

  -- `true` is "isolate me" without naming how. With a default it means the
  -- default; with nothing configured it has to refuse, or a toggle switched on
  -- against an unconfigured project silently produces an unsandboxed session.
  it("treats true as the default profile", function()
    start_isolated_session("iso-true", true, fresh_ws())
    assert_ran_image("alpine:latest")
  end)

  it("refuses true when nothing is configured to isolate with", function()
    spec.setup({})
    local ok, err = pcall(start_isolated_session, "iso-true-unconfigured", true, fresh_ws())
    assert.falsy(ok)
    assert.truthy(tostring(err):find("asked to be isolated", 1, true))
  end)

  -- `runtime` describes the box, not the image. A profile that omits it must
  -- not silently re-detect and ignore a deliberately configured runtime.
  it("inherits runtime and exempt from the top-level config into a profile", function()
    available = { podman = true, docker = true }
    start_isolated_session("iso-inherit", "heavy", fresh_ws())
    assert.equals("podman", exec_call("run").cmd)
    assert.equals("read_note", isolation_calls[#isolation_calls].exempt[1])
  end)

  it("lets a profile override the runtime it inherits", function()
    available = { podman = true, docker = true }
    start_isolated_session("iso-override-rt", "other_runtime", fresh_ws())
    assert.equals("docker", exec_call("run").cmd)
  end)

  -- One container per workspace is the sharing rule; it cannot also hand a
  -- session an image it did not ask for.
  it("refuses a session asking for a different image on an already-sandboxed workspace", function()
    local ws = fresh_ws()
    responders["podman inspect"] = { success = true, exit_code = 0, stdout = "true\n", stderr = "" }
    start_isolated_session("iso-owner", nil, ws)

    local ok, err = pcall(start_isolated_session, "iso-intruder", "heavy", ws)
    assert.falsy(ok, "joining would silently give this session an image it did not ask for")
    assert.truthy(tostring(err):find("heavy:latest", 1, true))
    assert.truthy(tostring(err):find("alpine:latest", 1, true))
  end)
end)

-- The mount target is resolved per session and carried on the container it
-- resolved to, because a devcontainer's `workspaceFolder` is typically
-- /workspaces/<name> rather than /workspace. Every place that names a path
-- inside the container — the bind mount, `-w`, the remapped tool paths, the
-- glob and grep search roots — has to read that one value or they disagree and
-- the agent works in a directory the mount is not at.
describe("oci mount target", function()
  local ws_seq = 0
  local function fresh_ws()
    ws_seq = ws_seq + 1
    return "/home/user/target" .. ws_seq
  end

  before_each(function()
    install_shell()
    exec_log = {}
    responders = {}
    isolation_calls = {}
    status_calls = {}
    clear_status_calls = {}
    available = { podman = true }
  end)

  it("defaults to /workspace when nothing configures one", function()
    spec.setup({ image = "alpine:latest" })
    local ws = fresh_ws()
    start_session("s-target-default", ws)

    local run = exec_call("run")
    assert.is_not_nil(index_of(run.args, ws .. ":/workspace:rw,z"))
    local w = index_of(run.args, "-w")
    assert.equals("/workspace", run.args[w + 1])
  end)

  it("mounts and works in a configured workspace_folder", function()
    spec.setup({ image = "alpine:latest", workspace_folder = "/workspaces/app" })
    local ws = fresh_ws()
    start_session("s-target-cfg", ws)

    local run = exec_call("run")
    assert.is_not_nil(index_of(run.args, ws .. ":/workspaces/app:rw,z"))
    local w = index_of(run.args, "-w")
    assert.equals("/workspaces/app", run.args[w + 1])
  end)

  it("lets a profile choose its own workspace_folder", function()
    spec.setup({
      image = "alpine:latest",
      profiles = { dev = { image = "dev:latest", workspace_folder = "/src" } },
    })
    local ws = fresh_ws()
    start_isolated_session("s-target-profile", "dev", ws)

    local run = exec_call("run")
    assert.is_not_nil(index_of(run.args, ws .. ":/src:rw,z"))
  end)

  it("runs bash in the resolved target, not /workspace", function()
    spec.setup({ image = "alpine:latest", workspace_folder = "/workspaces/app" })
    local ws = fresh_ws()
    start_session("s-target-bash", ws)
    exec_log = {}

    responders["podman exec"] = { success = true, exit_code = 0, stdout = "hi\n", stderr = "" }
    hooks.bash.fn({ session_id = "s-target-bash" }, { tool = "bash", args = { command = "echo hi" } })

    local call = exec_call("exec")
    local w = index_of(call.args, "-w")
    assert.is_not_nil(w)
    assert.equals("/workspaces/app", call.args[w + 1])
  end)

  it("remaps tool paths under the resolved target", function()
    spec.setup({ image = "alpine:latest", workspace_folder = "/workspaces/app" })
    local ws = fresh_ws()
    start_session("s-target-read", ws)
    exec_log = {}

    responders["podman exec"] = { success = true, exit_code = 0, stdout = "     1\tline\n", stderr = "" }
    hooks.read_file.fn(
      { session_id = "s-target-read" },
      { tool = "read_file", args = { path = ws .. "/src/main.rs" } }
    )

    local call = exec_call("exec")
    local script = call.args[index_of(call.args, "sh") + 2]
    assert.truthy(script:find("/workspaces/app/src/main.rs", 1, true))
    assert.falsy(script:find("/workspace/", 1, true))
  end)

  it("searches from the resolved target when glob and grep are given no path", function()
    spec.setup({ image = "alpine:latest", workspace_folder = "/workspaces/app" })
    local ws = fresh_ws()
    start_session("s-target-search", ws)
    exec_log = {}

    hooks.glob.fn({ session_id = "s-target-search" }, { tool = "glob", args = { pattern = "*.rs" } })
    local glob_script = exec_call("exec").args[index_of(exec_call("exec").args, "sh") + 2]
    assert.truthy(glob_script:find("'/workspaces/app'", 1, true))

    exec_log = {}
    hooks.grep.fn({ session_id = "s-target-search" }, { tool = "grep", args = { pattern = "todo" } })
    local grep_script = exec_call("exec").args[index_of(exec_call("exec").args, "sh") + 2]
    assert.truthy(grep_script:find("'/workspaces/app'", 1, true))
  end)

  -- Same rule as the image check: one container per workspace means a session
  -- asking for a different mount target on an already-sandboxed workspace
  -- cannot have it, and joining would silently give it a target it did not ask
  -- for.
  it("refuses a session asking for a different target on an already-sandboxed workspace", function()
    spec.setup({
      image = "alpine:latest",
      profiles = { elsewhere = { image = "alpine:latest", workspace_folder = "/elsewhere" } },
    })
    responders["podman inspect"] = { success = true, exit_code = 0, stdout = "true\n", stderr = "" }
    local ws = fresh_ws()
    start_session("s-target-owner", ws)

    local ok, err = pcall(start_isolated_session, "s-target-intruder", "elsewhere", ws)
    assert.falsy(ok)
    assert.truthy(tostring(err):find("/elsewhere", 1, true))
    assert.truthy(tostring(err):find("/workspace", 1, true))
  end)
end)

-- Documented options that resolve_config dropped: whatever was configured, the
-- runtime only ever saw 900/300.
describe("oci configured timeouts", function()
  local ws_seq = 0
  local function fresh_ws()
    ws_seq = ws_seq + 1
    return "/home/user/timeout" .. ws_seq
  end

  before_each(function()
    install_shell()
    exec_log = {}
    responders = {}
    isolation_calls = {}
    status_calls = {}
    clear_status_calls = {}
    available = { podman = true }
  end)

  it("passes a configured start_timeout to the run", function()
    spec.setup({ image = "alpine:latest", start_timeout = 60 })
    start_session("s-start-timeout", fresh_ws())
    assert.equals(60, exec_call("run").opts.timeout)
  end)

  it("passes a configured build_timeout to the build", function()
    spec.setup({ image = "alpine:latest", dockerfile = "Dockerfile", build_timeout = 1800 })
    start_session("s-build-timeout", fresh_ws())
    assert.equals(1800, exec_call("build").opts.timeout)
  end)

  it("falls back to the documented defaults when neither is configured", function()
    spec.setup({ image = "alpine:latest", dockerfile = "Dockerfile" })
    start_session("s-default-timeouts", fresh_ws())
    assert.equals(900, exec_call("build").opts.timeout)
    assert.equals(300, exec_call("run").opts.timeout)
  end)
end)

-- The devcontainer leg of the resolution order, driven through the real
-- init.lua. Resolution is a pure decision, so nothing here needs a runtime
-- installed — the stubbed shell stands in for one.
describe("oci devcontainer resolution", function()
  local ws_seq = 0
  local function fresh_ws()
    ws_seq = ws_seq + 1
    return "/home/user/dc" .. ws_seq
  end

  local saved_fs

  --- Put a devcontainer.json at `<ws>/.devcontainer/devcontainer.json`.
  ---
  --- Restored in after_each: `cru.fs` is the shared harness mock and a stub
  --- left installed would leak into whatever runs next.
  --- `head_body` defaults to `body`: the ordinary case is a devcontainer whose
  --- working tree matches what is committed. Pass it to model an edit that has
  --- not been committed, which resolution ignores.
  local function with_devcontainer(ws, body, present, head_body)
    head_body = head_body or body
    local files = { [ws .. "/.devcontainer/devcontainer.json"] = body }
    local found = { podman = true }
    for _, name in ipairs(present or {}) do found[name] = true end
    cru.fs = {
      exists = function(path) return files[path] ~= nil end,
      read = function(path)
        if files[path] == nil then error("File not found: " .. path) end
        return files[path]
      end,
    }
    available = found

    -- Resolution reads HEAD, not the working tree, so a devcontainer these
    -- tests want honoured has to be committed. That rule has its own coverage
    -- in devcontainer_test.lua; here it must not stand in the way of the
    -- parsing, CLI-deferral and lifecycle behaviour under test.
    responders["git -C"] = function(_, args)
      local verb, spec_arg = args[3], args[4] or ""
      if verb == "rev-parse" then
        return { success = true, exit_code = 0, stdout = ws .. "/.git\n", stderr = "" }
      elseif verb == "show" and spec_arg:match("devcontainer%.json$") then
        return { success = true, exit_code = 0, stdout = head_body, stderr = "" }
      end
      return { success = false, exit_code = 128, stdout = "",
               stderr = "fatal: path does not exist in 'HEAD'" }
    end
  end

  local function index_run(needle)
    local run = exec_call("run")
    assert.is_not_nil(run, "expected a container run")
    return index_of(run.args, needle), run
  end

  before_each(function()
    install_shell()
    -- init.lua decodes devcontainer.json with cru.json.decode, which
    -- install_shell() replaces with an encode-only stub.
    cru.json.decode = real_json.decode
    saved_fs = cru.fs
    exec_log = {}
    responders = {}
    isolation_calls = {}
    status_calls = {}
    clear_status_calls = {}
    available = { podman = true }
  end)

  after_each(function()
    cru.fs = saved_fs
  end)

  -- The plugin ships enabled in every install. Reading a devcontainer for a
  -- project that never asked for isolation would containerize any repo that
  -- happens to carry one — and refuse every session on a box with no runtime.
  it("ignores a devcontainer when nothing asked for isolation", function()
    spec.setup(nil)
    local ws = fresh_ws()
    with_devcontainer(ws, '{ "image": "dc:latest" }')
    start_session("dc-unasked", ws)
    assert.equals(0, #exec_log, "an unconfigured project must not containerize itself")
  end)

  it("uses the devcontainer when the project opts in", function()
    spec.setup({ devcontainer = true })
    local ws = fresh_ws()
    with_devcontainer(ws, '{ "image": "dc:latest" }')
    start_session("dc-optin", ws)
    assert.is_not_nil(index_run("dc:latest"))
  end)

  -- The design's order: the devcontainer is the project's environment, so it
  -- outranks a profile describing the same project.
  it("outranks the configured image", function()
    spec.setup({ image = "alpine:latest" })
    local ws = fresh_ws()
    with_devcontainer(ws, '{ "image": "dc:latest" }')
    start_session("dc-wins", ws)
    assert.is_not_nil(index_run("dc:latest"))
    assert.is_nil(index_run("alpine:latest"))
  end)

  -- ...but not an explicit session param, which is first in the order.
  it("yields to an inline session isolation object", function()
    spec.setup({ image = "alpine:latest" })
    local ws = fresh_ws()
    with_devcontainer(ws, '{ "image": "dc:latest" }')
    start_isolated_session("dc-inline", { image = "inline:latest" }, ws)
    assert.is_not_nil(index_run("inline:latest"))
  end)

  it("is not read at all when the session opted out", function()
    spec.setup({ image = "alpine:latest" })
    local ws = fresh_ws()
    with_devcontainer(ws, '{ "image": "dc:latest" }')
    start_isolated_session("dc-optout", false, ws)
    assert.equals(0, #exec_log)
  end)

  it("can be switched off so the profile wins", function()
    spec.setup({ image = "alpine:latest", devcontainer = false })
    local ws = fresh_ws()
    with_devcontainer(ws, '{ "image": "dc:latest" }')
    start_session("dc-off", ws)
    assert.is_not_nil(index_run("alpine:latest"))
  end)

  -- The devcontainer default is /workspaces/<name>, not the plugin's
  -- /workspace. Mounting at one and working in the other puts every relative
  -- tool call in an empty directory.
  it("mounts at the devcontainer's workspaceFolder", function()
    spec.setup({ image = "alpine:latest" })
    local ws = fresh_ws()
    with_devcontainer(ws, '{ "image": "dc:latest", "workspaceFolder": "/src" }')
    start_session("dc-target", ws)

    local i, run = index_run(ws .. ":/src:rw,z")
    assert.is_not_nil(i, "the bind mount must follow the devcontainer's workspaceFolder")
    assert.equals("/src", run.args[index_of(run.args, "-w") + 1])
  end)

  it("defaults the mount to /workspaces/<name> as the devcontainer spec does", function()
    spec.setup({ image = "alpine:latest" })
    local ws = fresh_ws()
    with_devcontainer(ws, '{ "image": "dc:latest" }')
    start_session("dc-default-target", ws)
    assert.is_not_nil(index_run(ws .. ":/workspaces/" .. ws:match("[^/]+$") .. ":rw,z"))
  end)

  -- Refused by default, because the agent can commit the file that asks for
  -- them. See devcontainer.HOST_KEYS.
  it("refuses a devcontainer asking for runArgs unless the operator opted in", function()
    spec.setup({ image = "alpine:latest" })
    local ws = fresh_ws()
    with_devcontainer(ws, '{ "image": "dc:latest", "runArgs": ["--privileged"] }')

    local ok, err = pcall(start_session, "dc-runargs-denied", ws)
    assert.falsy(ok, "a devcontainer reaching the host must not start a session")
    assert.truthy(tostring(err):find("devcontainer_host_access", 1, true), tostring(err))
    assert.is_nil(exec_call("run"), "and nothing may have been started")
  end)

  it("passes runArgs and remoteUser through to the run when host access is allowed", function()
    spec.setup({ image = "alpine:latest", devcontainer_host_access = true })
    local ws = fresh_ws()
    with_devcontainer(ws,
      '{ "image": "dc:latest", "runArgs": ["--cap-add", "SYS_PTRACE"], "remoteUser": "vscode" }')
    -- A root image, so the plugin's own uid pin does not compete for --user.
    responders["podman image"] = { success = true, exit_code = 0, stdout = "\n", stderr = "" }
    start_session("dc-runargs", ws)

    local i, run = index_run("--cap-add")
    assert.is_not_nil(i)
    assert.equals("SYS_PTRACE", run.args[i + 1])
    assert.equals("vscode", run.args[index_of(run.args, "--user") + 1])
  end)

  it("builds from build.dockerfile with the devcontainer directory as context", function()
    spec.setup({ image = "alpine:latest" })
    local ws = fresh_ws()
    with_devcontainer(ws, '{ "build": { "dockerfile": "Dockerfile" } }')
    start_session("dc-build", ws)

    local build = exec_call("build")
    assert.is_not_nil(build, "a devcontainer naming a Dockerfile must build it")
    assert.is_not_nil(index_of(build.args, ws .. "/.devcontainer/Dockerfile"))
    assert.is_not_nil(index_of(build.args, ws .. "/.devcontainer"))
  end)

  -- The refusal the whole feature turns on: an environment the plugin cannot
  -- reproduce is never quietly replaced with the one it can.
  it("refuses the session naming the key it cannot honour", function()
    spec.setup({ image = "alpine:latest" })
    local ws = fresh_ws()
    with_devcontainer(ws, '{ "image": "dc:latest", "features": { "x": {} } }')

    local ok, err = pcall(start_session, "dc-features", ws)
    assert.falsy(ok, "a devcontainer needing the CLI must refuse, not fall back to the profile")
    assert.truthy(tostring(err):find("features", 1, true))
    -- Nothing may be *started* from a config that could not be honoured. Reads
    -- are fine and expected: resolution consults git for the committed file
    -- before it can know the config is unhonourable at all.
    for _, call in ipairs(exec_log) do
      assert.equals("git", call.cmd,
        "a config that could not be honoured reached the container runtime")
    end
  end)

  -- A build takes minutes. `exec` returns nothing until it is over, so the
  -- slot said "building <image>" and then went silent — indistinguishable from
  -- a hang. Streaming is the only thing that fixes that; no status API can
  -- report what it was never told.
  it("reports build output as it arrives instead of going silent", function()
    spec.setup({ image = "built:latest" })
    local ws = fresh_ws()
    with_devcontainer(ws, '{ "build": { "dockerfile": "Dockerfile" } }')
    responders["podman build"] = {
      success = true, exit_code = 0,
      stdout = "STEP 1/3: FROM alpine\nSTEP 2/3: RUN apk add git\nSTEP 3/3: COMMIT\n",
      stderr = "",
    }

    start_session("dc-build-progress", ws)

    local progressed = {}
    for _, c in ipairs(status_calls) do
      if c.progress then progressed[#progressed + 1] = c.text end
    end
    assert.truthy(#progressed >= 3,
      "each build step must reach the status slot; got " .. #progressed)
    assert.truthy(progressed[1]:find("STEP 1/3", 1, true), progressed[1])
    assert.truthy(progressed[#progressed]:find("STEP 3/3", 1, true), progressed[#progressed])

    -- A build has no total to count against, so it spins rather than showing
    -- an invented fraction.
    for _, c in ipairs(status_calls) do
      if c.progress then assert.equals(true, c.progress) end
    end

    -- ...and the terminal slot is the sandbox state, with no progress on it.
    local final = status_calls[#status_calls]
    assert.truthy(final.text:find("sandboxed", 1, true), final.text)
    assert.is_nil(final.progress, "a finished build must not leave a live bar")
  end)

  -- HEAD is what runs, but a human who just edited the file and saw nothing
  -- change deserves to be told why rather than left to guess.
  it("says so in the status slot when the working tree differs from HEAD", function()
    spec.setup({ image = "alpine:latest" })
    local ws = fresh_ws()
    with_devcontainer(ws, '{ "image": "edited:latest" }', nil, '{ "image": "committed:latest" }')

    start_session("dc-drift", ws)

    local final = status_calls[#status_calls]
    assert.truthy(final.text:find("committed:latest", 1, true),
      "the committed image is the one that runs")
    assert.truthy(final.text:lower():find("uncommitted", 1, true),
      "an ignored working-tree edit must be visible, not silent")
    assert.equals("warn", final.level)
  end)

  it("keeps the status slot quiet when the working tree matches HEAD", function()
    spec.setup({ image = "alpine:latest" })
    local ws = fresh_ws()
    with_devcontainer(ws, '{ "image": "dc:latest" }')

    start_session("dc-clean", ws)

    local final = status_calls[#status_calls]
    assert.is_nil(final.text:lower():find("uncommitted", 1, true))
    assert.equals("info", final.level)
  end)

  -- With @devcontainers/cli present the environment is built by the tool that
  -- knows how, and the plugin adopts the container it produced.
  it("delegates to @devcontainers/cli when it is installed", function()
    spec.setup({ image = "alpine:latest" })
    local ws = fresh_ws()
    with_devcontainer(ws, '{ "image": "dc:latest", "features": { "x": {} } }', { "devcontainer" })
    responders["devcontainer up"] = {
      success = true, exit_code = 0, stderr = "",
      stdout = '{"outcome":"success","containerId":"dc-abc",'
        .. '"remoteWorkspaceFolder":"/workspaces/app"}\n',
    }
    start_session("dc-cli", ws)

    assert.is_nil(exec_call("run"), "the CLI creates the container, not the plugin")
    local up = exec_call("up")
    assert.is_not_nil(up)
    assert.equals("devcontainer", up.cmd)
    assert.is_not_nil(index_of(up.args, ws))
    assert.equals(1, #isolation_calls, "an adopted container is still a claimed sandbox")

    -- Tools route into the container the CLI reported, at the folder it reported.
    exec_log = {}
    responders["podman exec"] = { success = true, exit_code = 0, stdout = "hi\n", stderr = "" }
    hooks.bash.fn({ session_id = "dc-cli" }, { tool = "bash", args = { command = "echo hi" } })
    local call = exec_call("exec")
    assert.is_not_nil(index_of(call.args, "dc-abc"))
    assert.equals("/workspaces/app", call.args[index_of(call.args, "-w") + 1])
  end)

  it("refuses the session when devcontainer up fails", function()
    spec.setup({ image = "alpine:latest" })
    local ws = fresh_ws()
    with_devcontainer(ws, '{ "image": "dc:latest", "features": { "x": {} } }', { "devcontainer" })
    responders["devcontainer up"] = {
      success = false, exit_code = 1, stdout = "", stderr = "compose not found",
    }

    local ok, err = pcall(start_session, "dc-cli-fail", ws)
    assert.falsy(ok)
    assert.truthy(tostring(err):find("devcontainer up", 1, true))
    assert.equals(0, #isolation_calls)
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
    -- The container table is daemon-lifetime state keyed by workspace, so end
    -- the previous test's session before re-starting this one — otherwise the
    -- second `start_session` would join a container the stub never kept alive.
    lifecycle.end_fn({ id = "s-tools" })
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

-- ─────────────────────────────────────────────────────────────────────────────
-- What the plugin tells clients it offers
-- ─────────────────────────────────────────────────────────────────────────────

--- Clients used to answer "does this box offer isolation, and under what
--- names?" by reaching into raw `[plugins.oci]` TOML and matching on its shape,
--- which put this plugin's config schema in `crucible-web`. The plugin is the
--- only thing that knows what its own config means, so it says so directly.
--- Declared once, rendered by the TUI and the web settings pane alike. The
--- alternative was every frontend hand-building a container settings form and
--- drifting from this plugin's actual config keys.
describe("oci declares its settings", function()
  before_each(function()
    install_shell()
    declared_options = nil
  end)

  it("declares a settings tree at setup", function()
    spec.setup({ image = "alpine:latest" })
    assert.is_not_nil(declared_options, "no options declared; nothing to render")
    assert.equals("group", declared_options.type)
    assert.is_not_nil(declared_options.args.image)
    assert.is_not_nil(declared_options.args.runtime)
  end)

  -- One accessor at the root serves every leaf, routed by `info.option`.
  it("reads and writes config through the inherited accessors", function()
    spec.setup({ image = "alpine:latest" })
    local info = { option = "image" }
    assert.equals("alpine:latest", declared_options.get(info))

    declared_options.set(info, "debian:trixie")
    assert.equals("debian:trixie", declared_options.get(info))
  end)

  -- The reason `values` is a function: it describes this box, not this file.
  it("offers only the runtimes actually installed here", function()
    spec.setup({ image = "alpine:latest" })
    available = { podman = true }
    local only_podman = declared_options.args.runtime.values()
    assert.equals(1, #only_podman)
    assert.equals("podman", only_podman[1])

    available = { podman = true, docker = true }
    assert.equals(2, #declared_options.args.runtime.values(),
      "a runtime installed after load must appear without a reload")
  end)

  -- A button is a settings node, not a separate contribution API.
  it("offers orphan cleanup as a button that runs", function()
    spec.setup({ image = "alpine:latest" })
    local cleanup = declared_options.args.cleanup
    assert.equals("execute", cleanup.type)
    exec_log = {}
    cleanup.func({})
    assert.truthy(#exec_log > 0, "the button must actually do something")
  end)
end)

describe("oci publishes what it offers", function()
  before_each(function()
    install_shell()
    publications = {}
  end)

  it("offers isolation when an image is configured", function()
    spec.setup({ image = "alpine:latest" })
    assert.truthy(publications.isolation, "nothing published; clients have nothing to render")
    assert.truthy(publications.isolation.available)
  end)

  it("offers isolation when only named profiles are configured", function()
    spec.setup({ profiles = { rust = { image = "rust:1-bookworm" } } })
    assert.truthy(publications.isolation.available)
  end)

  it("offers isolation when the project's devcontainer is opted into", function()
    spec.setup({ devcontainer = true })
    assert.truthy(publications.isolation.available)
  end)

  -- A runtime says *how* to run a container, not that there is one to run.
  it("offers nothing when nothing is configured to run", function()
    spec.setup({ runtime = "podman" })
    assert.falsy(publications.isolation.available)
    spec.setup(nil)
    assert.falsy(publications.isolation.available)
  end)

  it("names the profiles, sorted, and nothing about their images", function()
    spec.setup({
      image = "alpine:latest",
      profiles = {
        throwaway = { image = "debian:trixie" },
        rust = { image = "rust:1-bookworm" },
      },
    })
    local names = publications.isolation.profiles
    assert.equals(2, #names)
    assert.equals("rust", names[1])
    assert.equals("throwaway", names[2])
    assert.is_nil(publications.isolation.images,
      "a profile's image is server-side detail; only the name is offered")
  end)

  -- An empty Lua table is ambiguous and encodes as `{}`, not `[]`. A client
  -- iterating that throws, and the whole offer — availability included —
  -- disappears with it. Omitting the key says "no named profiles" in a way
  -- that survives the encoding.
  it("omits the profile list entirely rather than publishing an empty table", function()
    spec.setup({ image = "alpine:latest" })
    assert.equals(true, publications.isolation.available,
      "a bare image is still an offer, with or without named profiles")
    assert.is_nil(publications.isolation.profiles)
  end)
end)
