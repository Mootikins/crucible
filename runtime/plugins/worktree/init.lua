--- Worktree Plugin — the workspace axis.
---
--- Answers *where do a session's files live?* by resolving a branch name to a
--- checkout, creating the worktree if there isn't one. The runtime axis —
--- *where does the process run?* — is a separate question answered by `oci` and
--- `ssh`, and the two compose: a session can run in a container against a
--- worktree.
---
--- This plugin therefore never claims isolation and never sets a sandbox exec.
--- It moves a path and stops. That separation is not tidiness: `session.isolation`
--- is one opaque value every isolating plugin interprets for itself, and `oci`
--- raises on a name it does not recognise, so a branch name sent down that
--- channel is a hard error in an unrelated plugin.
---
--- The daemon knows nothing about git — every worktree decision is made here,
--- the way every container decision is made in `oci`. What this replaces: the
--- `scm.branches` and `scm.worktree_add` RPCs, their `add_worktree` and
--- `collect_branches` implementations, and the orchestration the web composer
--- used to carry.

local git = require("git")

-- Populated by setup() from `[plugins.worktree]`. Empty rather than nil:
-- setup() is not called when there is no config section at all, and resolution
-- reads sub-keys unconditionally.
local config = {}

--- Run `git <args>` and return trimmed stdout, or nil + stderr.
---
--- Argv, never a shell string: a branch name is attacker-adjacent input (it can
--- come from a remote) and one containing a space, a quote or a `;` must reach
--- git as one argument.
--- Parenthesised because `gsub` returns the string AND a substitution count,
--- and a caller writing `local _, err = git_out(...)` would otherwise bind
--- `err` to that count — which is `0`, and `0` is truthy in Lua. Every call
--- would read as a failure.
local function git_out(args)
  local r = cru.shell.exec("git", args)
  if not r.success then
    return nil, ((r.stderr or ""):gsub("%s+$", ""))
  end
  return ((r.stdout or ""):gsub("%s+$", ""))
end

--- The workdir of the checkout containing `path`.
---
--- `--show-toplevel` resolves to the *linked worktree's* workdir when run
--- inside one, not the main checkout — which is what makes `is_current`
--- correct for a session already working in a worktree.
local function workdir(path)
  if not path or path == "" then return nil end
  return git_out({ "-C", path, "rev-parse", "--show-toplevel" })
end

--- Everything the branch list needs, in one place.
---
--- Returns `dir, porcelain, head, locals, remotes` or nil when `path` is not a
--- repository — which is not an error: plenty of projects are not repos, and
--- the chip simply offers nothing for them.
local function survey(path)
  local dir = workdir(path)
  if not dir then return nil end
  local porcelain = git_out({ "-C", dir, "worktree", "list", "--porcelain" })
  if not porcelain then return nil end
  local head = git_out({ "-C", dir, "rev-parse", "--abbrev-ref", "HEAD" }) or ""
  local locals = git_out({ "-C", dir, "for-each-ref", "refs/heads", "--format=%(refname:short)" }) or ""
  local remotes = git_out({ "-C", dir, "for-each-ref", "refs/remotes", "--format=%(refname:short)" }) or ""
  return dir, porcelain, head, locals, remotes
end

--- Create the worktree for `branch` and answer with its path.
---
--- `create` is inferred rather than asked for: a branch that exists is checked
--- out, one that does not is created. The old flow made the caller decide, and
--- the caller's only way to know was the same branch list this plugin already
--- has — so it was a question the UI had to answer wrongly or ask the user.
local function create_worktree(repo, branch, exists)
  local dest = git.worktree_dest(config.template, repo, branch)
  if cru.fs.exists(dest) then
    -- Already there. Idempotent on purpose: two sessions asking for the same
    -- branch is the parallel-agents flow, not a mistake.
    return dest
  end

  local args = { "-C", repo, "worktree", "add" }
  if exists then
    args[#args + 1] = dest
    args[#args + 1] = branch
  else
    args[#args + 1] = "-b"
    args[#args + 1] = branch
    args[#args + 1] = dest
  end

  local _, err = git_out(args)
  if err then
    error("worktree: could not create a worktree for '" .. branch .. "': " .. err)
  end

  -- A worktree inside the repo that is not ignored shows up as untracked, and
  -- the agent's next `git status` is then full of its own checkout. Warned, not
  -- refused: the template is the user's choice.
  if dest:sub(1, #repo) == repo then
    local r = cru.shell.exec("git", { "-C", repo, "check-ignore", "-q", dest })
    if not r.success then
      cru.log("warn", "worktree: " .. dest .. " is inside the repo but not ignored; "
        .. "add it to .gitignore or set [plugins.worktree] template")
    end
  end

  return dest
end

return {
  name = "worktree",
  version = "0.1.0",
  description = "Run a session against a git worktree — the workspace axis",

  commands = {
    --- Branches this project offers, as menu rows.
    ---
    --- A command rather than published data because the answer depends on which
    --- project is selected and on what has happened in the repo since — a branch
    --- created in a terminal has to show up without reloading the plugin.
    ["worktree.targets"] = {
      desc = "Branches available as workspace targets for a project",
      fn = function(args)
        local dir, porcelain, head, locals, remotes = survey(args and args.workspace)
        if not dir then return { targets = {} } end
        local branches = git.build_branches(porcelain, head, locals, remotes)
        return { targets = git.to_targets(branches) }
      end,
    },

    --- Resolve a branch to the checkout a session should run in.
    ---
    --- Called by the daemon before `session.create`, so raising here refuses the
    --- session. That is the intent: an agent that quietly works on `main` when
    --- it was told `feat/x` commits there.
    ["worktree.resolve"] = {
      desc = "Resolve a branch to its worktree path, creating one if needed",
      fn = function(args)
        local branch = args and args.target
        if not branch or branch == "" then
          error("worktree: no branch named")
        end
        local ok, why = git.validate_branch(branch)
        if not ok then
          error("worktree: refusing branch name '" .. branch .. "' (" .. why .. ")")
        end

        local dir, porcelain, head, locals, remotes = survey(args and args.workspace)
        if not dir then
          error("worktree: '" .. tostring(args and args.workspace)
            .. "' is not a git repository, so it has no worktrees")
        end

        -- `check-ref-format` on top of the rules above: git's own validation
        -- knows things this file should not have to restate.
        local _, bad = git_out({ "check-ref-format", "--branch", branch })
        if bad then
          error("worktree: git rejected the branch name '" .. branch .. "': " .. bad)
        end

        local repo = git.main_worktree(porcelain) or dir
        local branches = git.build_branches(porcelain, head, locals, remotes)

        for _, b in ipairs(branches) do
          if b.name == branch then
            -- Already checked out somewhere: jump to it rather than failing or
            -- making a second one. git refuses two worktrees on one branch, so
            -- this is the only answer that works.
            if b.worktree_path then return { path = b.worktree_path } end
            return { path = create_worktree(repo, branch, not b.remote_only) }
          end
        end

        -- A name no branch has yet is a new branch, cut from HEAD.
        return { path = create_worktree(repo, branch, false) }
      end,
    },
  },

  --- Receives the `[plugins.worktree]` section at load.
  setup = function(cfg)
    config = cfg or {}

    -- Declares the provider, not the targets. What this plugin *is* is stable;
    -- what it offers is not, so the list is fetched through `targets_command`.
    crucible.publish("targets", {
      axis = "workspace",
      label = "Worktree",
      targets_command = "worktree.targets",
      resolve_command = "worktree.resolve",
    })

    crucible.options{
      type = "group",
      name = "Worktrees",
      get = function(info) return config[info.option] end,
      set = function(info, value) config[info.option] = value end,
      args = {
        template = {
          type = "input", order = 1, name = "Worktree location",
          desc = "Where a new worktree goes. {repo} is the repository root and "
            .. "{branch} the branch name. Default: {repo}/tree/{branch}.",
        },
      },
    }
  end,
}
