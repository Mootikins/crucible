--- Pure path/text helpers for routing tool calls into a container.
--
-- Extracted from init.lua so tests exercise the same code the plugin runs.
-- The previous suite tested a hand-copied duplicate of remap_path, which
-- passed no matter how broken the plugin was.
local M = {}

--- Where the workspace is mounted when nothing resolves a target.
---
--- Defined once and shared with `container.run_args`: the bind target, the
--- container's working directory and the paths tools are handed all have to
--- name the same directory, and two copies of this string are two chances for
--- them not to.
M.DEFAULT_TARGET = "/workspace"

--- Map a host path into the container's workspace mount.
---
--- `target` is where that mount lives inside the container — /workspace by
--- default, but a devcontainer's `workspaceFolder` is typically
--- /workspaces/<name>, and a path remapped against the wrong root names a file
--- the container does not have.
function M.remap_path(workspace_host, path, target)
  target = target or M.DEFAULT_TARGET
  if not path then return target end
  if path:sub(1, #workspace_host) == workspace_host then
    local suffix = path:sub(#workspace_host + 1)
    if suffix == "" or suffix == "/" then return target end
    if suffix:sub(1, 1) == "/" then suffix = suffix:sub(2) end
    return target .. "/" .. suffix
  elseif path:sub(1, 1) == "/" then
    return path -- outside workspace, pass through
  else
    return target .. "/" .. path -- relative
  end
end

--- Shell-escape a string for use inside single quotes.
function M.sq(s)
  return s:gsub("'", "'\\''")
end

--- Format a list of lines with a count footer, truncating if over limit.
function M.truncate_lines(lines, limit, noun)
  local truncated = #lines > limit
  local kept = {}
  for i = 1, math.min(#lines, limit) do kept[i] = lines[i] end
  local suffix = truncated
    and string.format("\n\n[%d %s, truncated at %d]", #kept, noun, limit)
    or  string.format("\n\n[%d %s]", #kept, noun)
  return table.concat(kept, "\n") .. suffix
end

return M
