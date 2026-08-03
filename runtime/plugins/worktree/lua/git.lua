--- Pure git helpers for the worktree plugin.
---
--- Everything here is a string-in/string-out function so the tests exercise
--- the code the plugin runs rather than a copy. Nothing in this file shells
--- out; `init.lua` owns every `cru.shell.exec`.
---
--- Ported from `crucible-daemon/src/scm.rs`, which is what this plugin
--- replaces. The rules are the same rules — the point of the port is that
--- worktrees stop being a thing the daemon knows about, not that they start
--- behaving differently.

local M = {}

--- Reject a branch name before git ever sees it.
---
--- Layered on top of `git check-ref-format`, which `init.lua` still runs. These
--- come first so a name like `-foo` cannot be read as a flag by the
--- `check-ref-format` call itself, and so path-hostile names (`..`, `\`, a
--- leading `/`) are refused whatever git's own rules say about them.
---
--- Returns `true`, or `false, reason`.
function M.validate_branch(name)
  if type(name) ~= "string" or name == "" then return false, "empty" end
  if name:find("..", 1, true) then return false, "contains '..'" end
  if name:sub(1, 1) == "-" then return false, "starts with '-'" end
  if name:sub(1, 1) == "/" then return false, "starts with '/'" end
  if name:find("\\", 1, true) then return false, "contains '\\'" end
  return true
end

--- Where a new worktree goes, from a template (default `{repo}/tree/{branch}`).
---
--- A plain substitution, deliberately: `{branch}` keeps its slashes, so
--- `feat/x` nests a directory rather than being flattened. That matches what
--- the daemon did and what anyone reading the template expects.
function M.worktree_dest(template, repo, branch)
  local resolved = template or "{repo}/tree/{branch}"
  -- `gsub` with a plain replacement string would treat `%` in a path as a
  -- capture reference; the replacement is passed as a function to avoid it.
  resolved = resolved:gsub("{repo}", function() return repo end)
  resolved = resolved:gsub("{branch}", function() return branch end)
  return resolved
end

--- Parse `git worktree list --porcelain` into `{ [branch] = path }`.
---
--- Records are blank-line separated. A record has a `worktree <path>` line and
--- either a `branch refs/heads/<name>` line or a `detached` line — detached
--- worktrees map no branch and are skipped.
function M.parse_worktrees(porcelain)
  local map = {}
  local current = nil
  for line in (porcelain or ""):gmatch("[^\n]*") do
    local path = line:match("^worktree (.+)$")
    local branch = line:match("^branch (.+)$")
    if path then
      current = path
    elseif branch and current then
      map[(branch:gsub("^refs/heads/", ""))] = current
    elseif line == "" then
      current = nil
    end
  end
  return map
end

--- The main checkout's path: the first `worktree` record. git always lists it
--- first, which is the only reason this can be a one-liner.
function M.main_worktree(porcelain)
  return (porcelain or ""):match("^worktree (.-)\n") or (porcelain or ""):match("^worktree (.+)$")
end

--- `origin/feat/x` → `feat/x`, or nil for the symbolic `<remote>/HEAD` entries,
--- which name no branch.
function M.strip_remote(short)
  local _, branch = (short or ""):match("^([^/]+)/(.+)$")
  if not branch or branch == "HEAD" then return nil end
  return branch
end

--- Split command output into non-empty lines.
function M.lines(text)
  local out = {}
  for line in (text or ""):gmatch("[^\n]+") do
    if line ~= "" then out[#out + 1] = line end
  end
  return out
end

--- Order branches: current first, then those that already have a worktree,
--- then the rest — alphabetical within each group.
---
--- The ordering is the menu's whole usability story: the branch you are on and
--- the ones you can jump to without creating anything belong at the top, and
--- everything else is a longer action.
function M.sort_branches(branches)
  local rank = function(b)
    if b.is_current then return 0 end
    if b.worktree_path then return 1 end
    return 2
  end
  table.sort(branches, function(a, b)
    if rank(a) ~= rank(b) then return rank(a) < rank(b) end
    return a.name < b.name
  end)
  return branches
end

--- Build the branch list from raw git output.
---
--- Separated from the shelling out so the assembly — which branch is current,
--- which already has a worktree, which exists only on a remote — is testable
--- against fixed strings instead of a real repository.
function M.build_branches(porcelain, head, locals, remotes)
  local worktrees = M.parse_worktrees(porcelain)
  local current = (head ~= "HEAD" and head ~= "" and head) or nil

  local seen, branches = {}, {}
  for _, name in ipairs(M.lines(locals)) do
    seen[name] = true
    branches[#branches + 1] = {
      name = name,
      worktree_path = worktrees[name],
      is_current = name == current,
      remote_only = false,
    }
  end
  for _, short in ipairs(M.lines(remotes)) do
    local name = M.strip_remote(short)
    -- A remote branch that also exists locally is the same branch; listing it
    -- twice would offer two rows that do the same thing.
    if name and not seen[name] then
      seen[name] = true
      branches[#branches + 1] = {
        name = name,
        worktree_path = nil,
        is_current = false,
        remote_only = true,
      }
    end
  end

  return M.sort_branches(branches), current
end

--- One menu row per branch, in the shape the composer's chips read.
---
--- The hint is the whole affordance: it says whether picking this row jumps to
--- a checkout that exists or creates one, before the user commits to it.
function M.to_targets(branches)
  local targets = {}
  for _, b in ipairs(branches) do
    local hint
    if b.is_current then
      hint = "current"
    elseif b.worktree_path then
      hint = b.worktree_path:match("([^/]+)$")
    elseif b.remote_only then
      hint = "remote · new worktree"
    else
      hint = "new worktree"
    end
    targets[#targets + 1] = { value = b.name, label = b.name, hint = hint }
  end
  return targets
end

return M
