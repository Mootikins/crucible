-- Integration tests for the worktree plugin entry point.
-- Run with: cru plugin test runtime/plugins/worktree
--
-- Loads the REAL init.lua against a scripted `git`, so these fail if the plugin
-- stops publishing its provider, stops declaring its commands, or starts
-- shelling out differently — including the argv, which is where a branch name
-- containing a space or a `;` would otherwise become two arguments.

local publications = {}
local declared_options

crucible = crucible or {}
crucible.publish = function(key, value) publications[key] = value end
crucible.options = function(tree) declared_options = tree end

-- Scripted git. Responders are keyed by the first two argv words after the
-- `-C <dir>` pair, which is what actually distinguishes the calls; every call
-- is logged so a test can assert the exact argv.
local exec_log = {}
local responders = {}
local existing_paths = {}

--- The subcommand of a git argv, skipping any leading `-C <dir>`.
local function subcommand(args)
  local i = (args[1] == "-C") and 3 or 1
  return (args[i] or "") .. " " .. (args[i + 1] or "")
end

cru = cru or {}
cru.log = function() end
cru.fs = cru.fs or {}
cru.fs.exists = function(path) return existing_paths[path] == true end
cru.shell = cru.shell or {}
cru.shell.exec = function(cmd, args, opts)
  table.insert(exec_log, { cmd = cmd, args = args, opts = opts })
  local key = subcommand(args)
  local r = responders[key]
  if type(r) == "function" then return r(args) end
  if r then return r end
  return { success = true, exit_code = 0, stdout = "", stderr = "" }
end

-- Loaded by plugin name via the parent's `?/init.lua` path entry — the same
-- resolution the daemon's plugin loader performs, so a require that only works
-- in tests cannot slip through.
local plugin = require("worktree")

--- A repository with master checked out at /repo and feat/x in a worktree.
local function a_repo()
  exec_log = {}
  existing_paths = {}
  responders = {
    ["rev-parse --show-toplevel"] = { success = true, stdout = "/repo\n" },
    ["worktree list"] = {
      success = true,
      stdout = table.concat({
        "worktree /repo",
        "branch refs/heads/master",
        "",
        "worktree /repo/tree/feat/x",
        "branch refs/heads/feat/x",
        "",
      }, "\n"),
    },
    ["rev-parse --abbrev-ref"] = { success = true, stdout = "master\n" },
    ["for-each-ref refs/heads"] = { success = true, stdout = "master\nfeat/x\n" },
    ["for-each-ref refs/remotes"] = { success = true, stdout = "origin/only-remote\n" },
    ["check-ref-format --branch"] = { success = true, stdout = "" },
    ["worktree add"] = { success = true, stdout = "" },
    -- `check-ignore -q` exits 0 when the path IS ignored.
    ["check-ignore -q"] = { success = true, stdout = "" },
  }
end

--- The argv of the first logged `git worktree add`, or nil.
local function worktree_add_argv()
  for _, call in ipairs(exec_log) do
    if subcommand(call.args) == "worktree add" then return call.args end
  end
end

local function index_of(list, needle)
  for i, v in ipairs(list) do
    if v == needle then return i end
  end
end

local targets = plugin.commands["worktree.targets"].fn
local resolve = plugin.commands["worktree.resolve"].fn

describe("the plugin's declaration", function()
  it("publishes itself on the workspace axis, not the runtime one", function()
    plugin.setup({})
    local decl = publications["targets"]
    assert.truthy(decl)
    -- The whole reason the axes are separate: on the runtime axis this would
    -- be offered as isolation, and a branch name sent down that channel is a
    -- hard error inside oci.
    assert.equals("workspace", decl.axis)
    assert.equals("worktree.targets", decl.targets_command)
    assert.equals("worktree.resolve", decl.resolve_command)
  end)

  it("declares both commands it published the names of", function()
    assert.truthy(plugin.commands["worktree.targets"])
    assert.truthy(plugin.commands["worktree.resolve"])
  end)

  it("never claims isolation — that is the other axis", function()
    assert.equals(nil, publications["isolation"])
  end)

  it("declares its template setting", function()
    plugin.setup({})
    assert.truthy(declared_options)
    assert.truthy(declared_options.args.template)
  end)
end)

describe("worktree.targets", function()
  it("offers every branch, current first", function()
    a_repo()
    local result = targets({ workspace = "/repo" })
    assert.equals("master", result.targets[1].value)
    assert.equals("current", result.targets[1].hint)
    assert.equals("feat/x", result.targets[2].value)
  end)

  -- Plenty of projects are not repositories. The chip offering nothing is the
  -- correct answer; raising would break session creation for all of them.
  it("offers nothing for a project that is not a repository", function()
    a_repo()
    responders["rev-parse --show-toplevel"] = { success = false, stderr = "not a git repository" }
    local result = targets({ workspace = "/not-a-repo" })
    assert.equals(0, #result.targets)
  end)

  it("offers nothing rather than raising when given no workspace", function()
    a_repo()
    assert.equals(0, #targets({}).targets)
  end)
end)

describe("worktree.resolve", function()
  it("jumps to a checkout that already exists instead of making another", function()
    a_repo()
    -- git refuses two worktrees on one branch, so reusing is the only answer
    -- that works — and it is what the parallel-agents flow depends on.
    assert.equals("/repo/tree/feat/x", resolve({ workspace = "/repo", target = "feat/x" }).path)
    assert.equals(nil, worktree_add_argv())
  end)

  it("creates a worktree for a local branch that has none", function()
    a_repo()
    responders["for-each-ref refs/heads"] = { success = true, stdout = "master\nfeat/x\nzebra\n" }
    local result = resolve({ workspace = "/repo", target = "zebra" })
    assert.equals("/repo/tree/zebra", result.path)

    local argv = worktree_add_argv()
    assert.truthy(argv)
    -- Checked out, not created: `-b` on an existing branch fails.
    assert.equals(nil, index_of(argv, "-b"))
    assert.equals("zebra", argv[#argv])
  end)

  it("creates the branch too when nothing has it yet", function()
    a_repo()
    local result = resolve({ workspace = "/repo", target = "brand-new" })
    assert.equals("/repo/tree/brand-new", result.path)

    local argv = worktree_add_argv()
    assert.truthy(index_of(argv, "-b"), "a branch that does not exist has to be created")
    assert.equals("brand-new", argv[index_of(argv, "-b") + 1])
  end)

  it("returns an existing destination without touching git", function()
    a_repo()
    existing_paths["/repo/tree/zebra"] = true
    assert.equals("/repo/tree/zebra", resolve({ workspace = "/repo", target = "zebra" }).path)
    assert.equals(nil, worktree_add_argv())
  end)

  -- Fail-closed. Resolution runs before session.create, so raising refuses the
  -- session — which is the point: an agent that quietly works on master when it
  -- was told feat/x commits there.
  it("refuses a branch name that would be read as a flag", function()
    a_repo()
    local ok, err = pcall(resolve, { workspace = "/repo", target = "-b" })
    assert.falsy(ok)
    assert.truthy(tostring(err):find("-b", 1, true))
  end)

  it("refuses a name git itself rejects", function()
    a_repo()
    responders["check-ref-format --branch"] = { success = false, stderr = "bad ref" }
    assert.falsy(pcall(resolve, { workspace = "/repo", target = "has space" }))
  end)

  it("refuses a project that is not a repository", function()
    a_repo()
    responders["rev-parse --show-toplevel"] = { success = false, stderr = "not a git repository" }
    assert.falsy(pcall(resolve, { workspace = "/nope", target = "main" }))
  end)

  it("refuses when git could not create the worktree", function()
    a_repo()
    responders["worktree add"] = { success = false, stderr = "fatal: destination busy" }
    assert.falsy(pcall(resolve, { workspace = "/repo", target = "brand-new" }))
  end)

  it("refuses when no branch was named", function()
    a_repo()
    assert.falsy(pcall(resolve, { workspace = "/repo", target = "" }))
    assert.falsy(pcall(resolve, { workspace = "/repo" }))
  end)

  -- A branch name can arrive from a remote, so it is attacker-adjacent. Passed
  -- as argv it is one argument however it is spelled; joined into a shell
  -- string it would not be.
  it("passes the branch to git as one argument", function()
    a_repo()
    responders["for-each-ref refs/heads"] = { success = true, stdout = "master\nfix; rm -rf /\n" }
    resolve({ workspace = "/repo", target = "fix; rm -rf /" })
    local argv = worktree_add_argv()
    assert.truthy(index_of(argv, "fix; rm -rf /"), "the name must survive as a single argv entry")
  end)

  it("honours a configured worktree location", function()
    a_repo()
    plugin.setup({ template = "/scratch/{branch}" })
    assert.equals("/scratch/brand-new", resolve({ workspace = "/repo", target = "brand-new" }).path)
    plugin.setup({})
  end)
end)
