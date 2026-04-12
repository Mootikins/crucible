--- OCI Container Plugin
-- Manages container lifecycle and tool interception for sandboxed workspace tool execution.
-- When a project has [container] config, this plugin:
-- 1. Creates a container on session start (sleep infinity sidecar pattern)
-- 2. Registers pre_tool_call handlers that intercept workspace tools (bash, read_file, etc.)
-- 3. Routes intercepted tool calls through `docker exec` inside the container
-- 4. Cleans up the container on session end
--
-- Zero Rust docker knowledge — all container logic lives here in Lua.
-- Uses generic crucible.on() hooks with pattern matching and the Handled result convention.

local container = require("lua.container")

local active = nil -- { name: string, runtime: string, workspace: string }

-- ─────────────────────────────────────────────────────────────────────────────
-- Path remapping
-- ─────────────────────────────────────────────────────────────────────────────

local function remap_path(workspace_host, path)
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

-- Shell-escape a string for use inside single quotes
local function sq(s)
  return s:gsub("'", "'\\''")
end

-- ─────────────────────────────────────────────────────────────────────────────
-- Tool handlers
-- ─────────────────────────────────────────────────────────────────────────────

local function handle_bash(ctx, event)
  if not active then return nil end
  local cmd = event.args and event.args.command or ""
  local timeout = (event.args and event.args.timeout_ms) or 120000

  local r = cru.shell.exec(active.runtime, {
    "exec", "-w", "/workspace", active.name, "sh", "-c", cmd,
  }, { timeout = math.floor(timeout / 1000) })

  local result = r.success and r.stdout or
    string.format("Exit code: %d\nStdout:\n%s\nStderr:\n%s",
      r.exit_code or -1, r.stdout or "", r.stderr or "")

  return { handled = true, result = cru.json.encode({ result = result }) }
end

local function handle_read_file(ctx, event)
  if not active then return nil end
  local path = remap_path(active.workspace, event.args and event.args.path)
  local offset = (event.args and event.args.offset) or 1
  local limit = event.args and event.args.limit

  local script
  if limit and limit > 0 then
    script = string.format("cat -n '%s' | tail -n +%d | head -n %d", sq(path), offset, limit)
  elseif offset > 1 then
    script = string.format("cat -n '%s' | tail -n +%d", sq(path), offset)
  else
    script = string.format("cat -n '%s'", sq(path))
  end

  local r = cru.shell.exec(active.runtime, { "exec", active.name, "sh", "-c", script })
  if not r.success then
    return { handled = true, result = cru.json.encode({ result = "Error: " .. (r.stderr or "read failed") }) }
  end
  return { handled = true, result = cru.json.encode({ result = r.stdout }) }
end

local function handle_write_file(ctx, event)
  if not active then return nil end
  local path = remap_path(active.workspace, event.args and event.args.path)
  local content = event.args and event.args.content or ""

  local script = string.format("mkdir -p \"$(dirname '%s')\" && cat > '%s'", sq(path), sq(path))
  local r = cru.shell.exec(active.runtime, {
    "exec", "-i", active.name, "sh", "-c", script,
  }, { stdin = content })

  if not r.success then
    return { handled = true, result = cru.json.encode({ result = "Error: " .. (r.stderr or "write failed") }) }
  end
  return { handled = true, result = cru.json.encode({
    result = string.format("Written %d bytes to %s", #content, event.args.path or path)
  }) }
end

local function handle_edit_file(ctx, event)
  if not active then return nil end
  local path = remap_path(active.workspace, event.args and event.args.path)
  local old_string = event.args and event.args.old_string or ""
  local new_string = event.args and event.args.new_string or ""
  local replace_all = event.args and event.args.replace_all

  -- Read current content via container
  local r = cru.shell.exec(active.runtime, { "exec", active.name, "cat", path })
  if not r.success then
    return { handled = true, result = cru.json.encode({ result = "Error: " .. (r.stderr or "read failed") }) }
  end

  local content = r.stdout
  if not content:find(old_string, 1, true) then
    return { handled = true, result = cru.json.encode({ result = "Error: old_string not found in file" }) }
  end

  local new_content, count
  if replace_all then
    new_content, count = content:gsub(old_string, new_string, nil)
    -- gsub with pattern; use plain replacement
    new_content = content
    count = 0
    local pos = 1
    while true do
      local s, e = content:find(old_string, pos, true)
      if not s then break end
      count = count + 1
      pos = e + 1
    end
    new_content = content:gsub(old_string:gsub("([%(%)%.%%%+%-%*%?%[%^%$])", "%%%1"), new_string)
  else
    local s, e = content:find(old_string, 1, true)
    new_content = content:sub(1, s - 1) .. new_string .. content:sub(e + 1)
    count = 1
  end

  -- Write back via container
  local write_script = string.format("cat > '%s'", sq(path))
  cru.shell.exec(active.runtime, {
    "exec", "-i", active.name, "sh", "-c", write_script,
  }, { stdin = new_content })

  return { handled = true, result = cru.json.encode({
    result = string.format("Replaced %d occurrence(s)", count)
  }) }
end

local function handle_glob(ctx, event)
  if not active then return nil end
  local pattern = event.args and event.args.pattern or "*"
  local path = event.args and event.args.path
  local limit = (event.args and event.args.limit) or 100

  local search_dir = path and remap_path(active.workspace, path) or "/workspace"

  local script
  if pattern:find("/") or pattern:find("%*%*") then
    local find_pattern = pattern:gsub("%*%*/", "*/")
    script = string.format("find '%s' -type f -path '*%s' 2>/dev/null | head -n %d",
      sq(search_dir), sq(find_pattern), limit + 1)
  else
    script = string.format("find '%s' -type f -name '%s' 2>/dev/null | head -n %d",
      sq(search_dir), sq(pattern), limit + 1)
  end

  local r = cru.shell.exec(active.runtime, { "exec", active.name, "sh", "-c", script })
  local lines = {}
  for line in (r.stdout or ""):gmatch("[^\n]+") do
    if line ~= "" then table.insert(lines, line) end
  end

  local truncated = #lines > limit
  local files = {}
  for i = 1, math.min(#lines, limit) do files[i] = lines[i] end

  local result
  if truncated then
    result = table.concat(files, "\n") .. string.format("\n\n[%d files, truncated at %d]", #files, limit)
  else
    result = table.concat(files, "\n") .. string.format("\n\n[%d files]", #files)
  end

  return { handled = true, result = cru.json.encode({ result = result }) }
end

local function handle_grep(ctx, event)
  if not active then return nil end
  local pattern = event.args and event.args.pattern or ""
  local path = event.args and event.args.path
  local glob_filter = event.args and event.args.glob
  local limit = (event.args and event.args.limit) or 50

  local search_path = path and remap_path(active.workspace, path) or "/workspace"

  -- Try rg first, fall back to grep -rn
  local script = "rg --line-number --max-count 1000 "
  if glob_filter then
    script = script .. string.format("--glob '%s' ", sq(glob_filter))
  end
  script = script .. string.format("'%s' '%s' 2>/dev/null || grep -rn '%s' '%s'",
    sq(pattern), sq(search_path), sq(pattern), sq(search_path))

  local r = cru.shell.exec(active.runtime, { "exec", active.name, "sh", "-c", script })
  local lines = {}
  for line in (r.stdout or ""):gmatch("[^\n]+") do
    table.insert(lines, line)
    if #lines > limit then break end
  end

  local truncated = #lines > limit
  local result_lines = {}
  for i = 1, math.min(#lines, limit) do result_lines[i] = lines[i] end

  local result
  if truncated then
    result = table.concat(result_lines, "\n") .. string.format("\n\n[%d matches, truncated at %d]", #result_lines, limit)
  else
    result = table.concat(result_lines, "\n") .. string.format("\n\n[%d matches]", #result_lines)
  end

  return { handled = true, result = cru.json.encode({ result = result }) }
end

-- ─────────────────────────────────────────────────────────────────────────────
-- Tool handler dispatch table
-- ─────────────────────────────────────────────────────────────────────────────

local TOOL_HANDLERS = {
  bash = handle_bash,
  read_file = handle_read_file,
  write_file = handle_write_file,
  edit_file = handle_edit_file,
  glob = handle_glob,
  grep = handle_grep,
}

-- ─────────────────────────────────────────────────────────────────────────────
-- Config resolution
-- ─────────────────────────────────────────────────────────────────────────────

local function resolve_config()
  local cfg = cru.config.get("container")
  if not cfg or not cfg.image then return nil end
  cfg.runtime = cfg.runtime or "docker"
  cfg.mounts = cfg.mounts or {}
  cfg.env = cfg.env or {}
  return cfg
end

-- ─────────────────────────────────────────────────────────────────────────────
-- Orphan cleanup
-- ─────────────────────────────────────────────────────────────────────────────

local function cleanup_orphans()
  local cfg = cru.config.get("container")
  local runtime = cfg and cfg.runtime or "docker"
  local containers = container.list_crucible(runtime)
  for _, c in ipairs(containers) do
    local session = cru.sessions and cru.sessions.get(c.session_id)
    if not session then
      cru.log("info", "Removing orphaned container: " .. c.name)
      container.rm(runtime, c.name)
    end
  end
end

pcall(cleanup_orphans)

-- ─────────────────────────────────────────────────────────────────────────────
-- Session lifecycle + tool registration
-- ─────────────────────────────────────────────────────────────────────────────

crucible.on_session_start(function(session)
  local cfg = resolve_config()
  if not cfg then return end

  local name = "crucible-" .. session.id
  local workspace = session.workspace or "."

  -- Build from Dockerfile if configured
  if cfg.dockerfile and cfg.dockerfile ~= "" then
    cru.log("info", "Building container image: " .. cfg.image)
    local r = container.build(cfg.runtime, {
      image = cfg.image,
      dockerfile = cfg.dockerfile,
      context = workspace,
    })
    if not r.success then
      cru.log("error", "Container build failed: " .. (r.stderr or "unknown error"))
      return
    end
  end

  -- Create and start container
  local r = container.run(cfg.runtime, {
    name = name,
    session_id = session.id,
    workspace = workspace,
    image = cfg.image,
    mounts = cfg.mounts,
    env = cfg.env,
  })

  if not r.success then
    cru.log("error", "Container start failed: " .. (r.stderr or "unknown error"))
    return
  end

  active = { name = name, runtime = cfg.runtime, workspace = workspace }
  cru.log("info", "Container started: " .. name .. " (" .. cfg.image .. ")")

  -- Register tool interception handlers for all workspace tools
  for tool_name, handler_fn in pairs(TOOL_HANDLERS) do
    crucible.on("pre_tool_call", { pattern = tool_name, priority = 10 }, handler_fn)
  end
end)

crucible.on_session_end(function(session)
  if not active then return end
  container.stop(active.runtime, active.name)
  container.rm(active.runtime, active.name)
  cru.log("info", "Container removed: " .. active.name)
  active = nil
end)

return {
  name = "oci",
  version = "0.2.0",
  description = "Run agent tools inside OCI containers via generic hook interception",
}
