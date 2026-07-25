--- Pure path/text helpers for routing tool calls into a container.
--
-- Extracted from init.lua so tests exercise the same code the plugin runs.
-- The previous suite tested a hand-copied duplicate of remap_path, which
-- passed no matter how broken the plugin was.
local M = {}

--- Map a host path into the container's /workspace mount.
function M.remap_path(workspace_host, path)
  if not path then return "/workspace" end
  if path:sub(1, #workspace_host) == workspace_host then
    local suffix = path:sub(#workspace_host + 1)
    if suffix == "" or suffix == "/" then return "/workspace" end
    if suffix:sub(1, 1) == "/" then suffix = suffix:sub(2) end
    return "/workspace/" .. suffix
  elseif path:sub(1, 1) == "/" then
    return path -- outside workspace, pass through
  else
    return "/workspace/" .. path -- relative
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
