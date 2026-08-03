-- Unit tests for the worktree plugin's pure git helpers.
-- Run with: cru plugin test runtime/plugins/worktree

local git = require("git")

describe("validate_branch", function()
  it("accepts ordinary branch names, slashes and all", function()
    assert.truthy(git.validate_branch("main"))
    assert.truthy(git.validate_branch("feat/container-isolation"))
    assert.truthy(git.validate_branch("release-0.20.1"))
  end)

  -- Checked before the name reaches git so `check-ref-format` cannot read it
  -- as one of its own options.
  it("refuses a name that would be read as a flag", function()
    local ok, why = git.validate_branch("-b")
    assert.falsy(ok)
    assert.equals("starts with '-'", why)
  end)

  -- The name becomes a path component in the worktree destination. `..` there
  -- escapes the template's directory entirely.
  it("refuses path traversal and separators", function()
    assert.falsy(git.validate_branch("../../etc/passwd"))
    assert.falsy(git.validate_branch("/absolute"))
    assert.falsy(git.validate_branch("back\\slash"))
  end)

  it("refuses an empty or absent name", function()
    assert.falsy(git.validate_branch(""))
    assert.falsy(git.validate_branch(nil))
  end)
end)

describe("worktree_dest", function()
  it("substitutes both placeholders", function()
    assert.equals("/repo/tree/feat/x", git.worktree_dest(nil, "/repo", "feat/x"))
  end)

  it("honours a configured template", function()
    assert.equals(
      "/scratch/crucible-feat/x",
      git.worktree_dest("/scratch/{branch}", "/repo", "crucible-feat/x")
    )
  end)

  -- `gsub`'s replacement string treats `%` as a capture reference, so a repo
  -- path containing one would corrupt the result or raise.
  it("survives a percent sign in the repo path", function()
    assert.equals("/tmp/100%/tree/main", git.worktree_dest(nil, "/tmp/100%", "main"))
  end)
end)

describe("parse_worktrees", function()
  local porcelain = table.concat({
    "worktree /repo",
    "HEAD abc123",
    "branch refs/heads/master",
    "",
    "worktree /repo/tree/feat/x",
    "HEAD def456",
    "branch refs/heads/feat/x",
    "",
    "worktree /repo/tree/detached",
    "HEAD 999999",
    "detached",
    "",
  }, "\n")

  it("maps each branch to its checkout", function()
    local map = git.parse_worktrees(porcelain)
    assert.equals("/repo", map["master"])
    assert.equals("/repo/tree/feat/x", map["feat/x"])
  end)

  -- A detached worktree names no branch, so it can be jumped to by no branch.
  it("skips detached worktrees", function()
    local map = git.parse_worktrees(porcelain)
    local count = 0
    for _ in pairs(map) do count = count + 1 end
    assert.equals(2, count)
  end)

  it("reports the main checkout as the first record", function()
    assert.equals("/repo", git.main_worktree(porcelain))
  end)

  it("survives empty input", function()
    assert.equals(0, #git.lines(""))
    local map = git.parse_worktrees("")
    assert.equals(nil, next(map))
  end)
end)

describe("strip_remote", function()
  it("drops the remote segment", function()
    assert.equals("feat/x", git.strip_remote("origin/feat/x"))
    assert.equals("main", git.strip_remote("upstream/main"))
  end)

  -- `origin/HEAD` is a symbolic ref, not a branch; offering it would produce a
  -- worktree on whatever HEAD happened to point at.
  it("refuses the symbolic HEAD entry", function()
    assert.equals(nil, git.strip_remote("origin/HEAD"))
  end)

  it("refuses a name with no remote segment", function()
    assert.equals(nil, git.strip_remote("main"))
  end)
end)

describe("build_branches", function()
  local porcelain = table.concat({
    "worktree /repo",
    "branch refs/heads/master",
    "",
    "worktree /repo/tree/feat/x",
    "branch refs/heads/feat/x",
    "",
  }, "\n")
  local locals = "master\nfeat/x\nzebra"
  local remotes = "origin/master\norigin/only-remote\norigin/HEAD"

  it("marks the current branch, and only it", function()
    local branches = git.build_branches(porcelain, "feat/x", locals, remotes)
    local current = {}
    for _, b in ipairs(branches) do
      if b.is_current then current[#current + 1] = b.name end
    end
    assert.equals(1, #current)
    assert.equals("feat/x", current[1])
  end)

  it("attaches an existing checkout to the branch that owns it", function()
    local branches = git.build_branches(porcelain, "master", locals, remotes)
    for _, b in ipairs(branches) do
      if b.name == "feat/x" then assert.equals("/repo/tree/feat/x", b.worktree_path) end
      if b.name == "zebra" then assert.equals(nil, b.worktree_path) end
    end
  end)

  -- A branch that exists both locally and on a remote is one branch. Listed
  -- twice, the menu offers two rows that do exactly the same thing.
  it("does not list a local branch again as remote-only", function()
    local branches = git.build_branches(porcelain, "master", locals, remotes)
    local seen = 0
    for _, b in ipairs(branches) do
      if b.name == "master" then seen = seen + 1 end
    end
    assert.equals(1, seen)
  end)

  it("includes a branch that exists only on a remote", function()
    local branches = git.build_branches(porcelain, "master", locals, remotes)
    local found
    for _, b in ipairs(branches) do
      if b.name == "only-remote" then found = b end
    end
    assert.truthy(found)
    assert.equals(true, found.remote_only)
  end)

  -- Current first, then branches you can jump to, then everything else: the
  -- rows that cost nothing are the rows at the top.
  it("orders current, then checked-out, then the rest", function()
    local branches = git.build_branches(porcelain, "master", locals, remotes)
    local names = {}
    for _, b in ipairs(branches) do names[#names + 1] = b.name end
    assert.equals("master", names[1])
    assert.equals("feat/x", names[2])
    assert.equals("only-remote", names[3])
    assert.equals("zebra", names[4])
  end)

  it("reports no current branch on a detached HEAD", function()
    local branches, current = git.build_branches(porcelain, "HEAD", locals, remotes)
    assert.equals(nil, current)
    for _, b in ipairs(branches) do assert.falsy(b.is_current) end
  end)
end)

describe("to_targets", function()
  -- The hint is what tells the user whether picking this row jumps somewhere
  -- that exists or creates a checkout, before they commit to it.
  it("says what picking each row will do", function()
    local targets = git.to_targets({
      { name = "master", is_current = true },
      { name = "feat/x", worktree_path = "/repo/tree/feat-x" },
      { name = "only-remote", remote_only = true },
      { name = "zebra" },
    })
    assert.equals("current", targets[1].hint)
    assert.equals("feat-x", targets[2].hint)
    assert.equals("remote · new worktree", targets[3].hint)
    assert.equals("new worktree", targets[4].hint)
  end)

  it("carries the branch name as the value the daemon resolves", function()
    local targets = git.to_targets({ { name = "feat/x" } })
    assert.equals("feat/x", targets[1].value)
    assert.equals("feat/x", targets[1].label)
  end)
end)
