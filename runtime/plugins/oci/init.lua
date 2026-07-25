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

local container = require("container")
local remap = require("remap")

-- Per session, because handlers are registered once at plugin load and are
-- shared by every session. A single global was only ever correct when
-- handlers were (re-)registered per session — which also made them
-- accumulate, one stale copy per session, for the daemon's lifetime.
local sessions = {} -- session_id -> { name, runtime, workspace }

--- The container serving this tool call, or nil if the session has none.
local function active_for(ctx)
  local id = ctx and ctx.session_id
  if not id then return nil end
  return sessions[id]
end

-- Pure helpers live in lua/remap.lua so tests exercise the same code the
-- plugin runs, not a copy.
local remap_path = remap.remap_path
local sq = remap.sq
local truncate_lines = remap.truncate_lines

-- ─────────────────────────────────────────────────────────────────────────────
-- Tool handlers
-- ─────────────────────────────────────────────────────────────────────────────

local function handle_bash(ctx, event)
  local active = active_for(ctx)
  if not active then return nil end
  local args = event.args or {}
  local cmd = args.command or ""
  local timeout = args.timeout_ms or 120000

  local r = cru.shell.exec(active.runtime, {
    "exec", "-w", "/workspace", active.name, "sh", "-c", cmd,
  }, { timeout = math.floor(timeout / 1000) })

  local result = r.success and r.stdout or
    string.format("Exit code: %d\nStdout:\n%s\nStderr:\n%s",
      r.exit_code or -1, r.stdout or "", r.stderr or "")

  return { handled = true, result = cru.json.encode({ result = result }) }
end

local function handle_read_file(ctx, event)
  local active = active_for(ctx)
  if not active then return nil end
  local args = event.args or {}
  local path = remap_path(active.workspace, args.path)
  local offset = args.offset or 1
  local limit = args.limit

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
  local active = active_for(ctx)
  if not active then return nil end
  local args = event.args or {}
  local path = remap_path(active.workspace, args.path)
  local content = args.content or ""

  local script = string.format("mkdir -p \"$(dirname '%s')\" && cat > '%s'", sq(path), sq(path))
  local r = cru.shell.exec(active.runtime, {
    "exec", "-i", active.name, "sh", "-c", script,
  }, { stdin = content })

  if not r.success then
    return { handled = true, result = cru.json.encode({ result = "Error: " .. (r.stderr or "write failed") }) }
  end
  return { handled = true, result = cru.json.encode({
    result = string.format("Written %d bytes to %s", #content, args.path or path)
  }) }
end

local function handle_edit_file(ctx, event)
  local active = active_for(ctx)
  if not active then return nil end
  local args = event.args or {}
  local path = remap_path(active.workspace, args.path)
  local old_string = args.old_string or ""
  local new_string = args.new_string or ""
  local replace_all = args.replace_all

  local r = cru.shell.exec(active.runtime, { "exec", active.name, "cat", path })
  if not r.success then
    return { handled = true, result = cru.json.encode({ result = "Error: " .. (r.stderr or "read failed") }) }
  end

  local content = r.stdout
  if not content:find(old_string, 1, true) then
    return { handled = true, result = cru.json.encode({ result = "Error: old_string not found in file" }) }
  end

  -- Escape Lua pattern metacharacters for safe gsub
  local escaped = old_string:gsub("([%(%)%.%%%+%-%*%?%[%^%$])", "%%%1")
  local escaped_replacement = new_string:gsub("%%", "%%%%")

  local new_content, count
  if replace_all then
    new_content, count = content:gsub(escaped, escaped_replacement)
  else
    local s, e = content:find(old_string, 1, true)
    new_content = content:sub(1, s - 1) .. new_string .. content:sub(e + 1)
    count = 1
  end

  local write_script = string.format("cat > '%s'", sq(path))
  cru.shell.exec(active.runtime, {
    "exec", "-i", active.name, "sh", "-c", write_script,
  }, { stdin = new_content })

  return { handled = true, result = cru.json.encode({
    result = string.format("Replaced %d occurrence(s)", count)
  }) }
end

local function handle_glob(ctx, event)
  local active = active_for(ctx)
  if not active then return nil end
  local args = event.args or {}
  local pattern = args.pattern or "*"
  local limit = args.limit or 100

  local search_dir = args.path and remap_path(active.workspace, args.path) or "/workspace"

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
    if line ~= "" then lines[#lines + 1] = line end
  end

  return { handled = true, result = cru.json.encode({
    result = truncate_lines(lines, limit, "files")
  }) }
end

local function handle_grep(ctx, event)
  local active = active_for(ctx)
  if not active then return nil end
  local args = event.args or {}
  local pattern = args.pattern or ""
  local glob_filter = args.glob
  local limit = args.limit or 50

  local search_path = args.path and remap_path(active.workspace, args.path) or "/workspace"

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
    lines[#lines + 1] = line
    if #lines > limit then break end
  end

  return { handled = true, result = cru.json.encode({
    result = truncate_lines(lines, limit, "matches")
  }) }
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

-- Populated by setup() from [plugins.oci]. `cru.config.get("container")`
-- never resolved: cru.config is the app-config store, so it read a top-level
-- `container` key that does not exist, resolve_config returned nil, and the
-- session silently started with no container at all.
local config = nil

local function resolve_config()
  if not config or not config.image then return nil end
  return {
    image = config.image,
    runtime = config.runtime,
    dockerfile = config.dockerfile,
    mounts = config.mounts or {},
    env = config.env or {},
    userns = config.userns,
    exempt = config.exempt or {},
  }
end

-- ─────────────────────────────────────────────────────────────────────────────
-- Orphan cleanup
-- ─────────────────────────────────────────────────────────────────────────────

local function cleanup_orphans(runtime)
  if not runtime then return end
  local containers = container.list_crucible(runtime)
  for _, c in ipairs(containers) do
    local session = cru.sessions and cru.sessions.get(c.session_id)
    if not session then
      cru.log("info", "Removing orphaned container: " .. c.name)
      container.rm(runtime, c.name)
    end
  end
end


-- ─────────────────────────────────────────────────────────────────────────────
-- Session lifecycle + tool registration
-- ─────────────────────────────────────────────────────────────────────────────

-- Handlers register ONCE, at load — not inside on_session_start.
--
-- The registry is append-only with no unregister, so registering per session
-- left one stale copy per session firing against a dead container for the
-- daemon's lifetime. Each handler resolves its own session via ctx.session_id
-- and no-ops when that session has no container.
for tool_name, handler_fn in pairs(TOOL_HANDLERS) do
  crucible.on("pre_tool_call", { pattern = tool_name, priority = 10 }, handler_fn)
end

--- `required = true`: a failure here refuses the session.
---
--- This is the whole contract. Previously a build or start failure logged one
--- ERROR line, skipped handler registration, and let the session run every
--- tool on the host — so "sandbox broken" and "sandbox working" were
--- indistinguishable from the outside. Refusing makes "session exists" imply
--- "session is sandboxed".
crucible.on_session_start(function(session)
  local cfg = resolve_config()
  -- Not configured is not a failure: no [plugins.oci] image means the user
  -- never asked for isolation, and every session would otherwise be refused.
  if not cfg then return end

  local runtime, detect_err = container.detect(cfg.runtime)
  if not runtime then
    error("oci: " .. detect_err)
  end

  pcall(cleanup_orphans, runtime)

  local name = "crucible-" .. session.id
  local workspace = session.workspace
  if not workspace or workspace == "" then
    error("oci: session has no workspace to isolate")
  end

  if cfg.dockerfile and cfg.dockerfile ~= "" then
    crucible.set_status{
      session = session.id, key = "oci", plugin = "oci",
      text = "building " .. cfg.image, level = "info",
    }
    local b = container.build(runtime, {
      image = cfg.image, dockerfile = cfg.dockerfile, context = workspace,
    })
    if not b.success then
      error("oci: image build failed: " .. (b.stderr or "unknown error"))
    end
  end

  crucible.set_status{
    session = session.id, key = "oci", plugin = "oci",
    text = "starting " .. cfg.image, level = "info",
  }

  -- Uid mapping only when the image runs as non-root; see container.lua for
  -- the measurements. Both keep-id and an explicit --user are required.
  local userns, run_as_uid, run_as_gid = cfg.userns, nil, nil
  if userns == nil and container.image_runs_as_non_root(runtime, cfg.image) then
    userns = "keep-id"
    run_as_uid, run_as_gid = container.host_ids()
    if not run_as_uid then
      error("oci: image runs as non-root but the host uid could not be read; "
        .. "workspace writes would fail with permission denied")
    end
  end

  local r = container.run(runtime, {
    name = name, session_id = session.id, workspace = workspace,
    image = cfg.image, mounts = cfg.mounts, env = cfg.env,
    userns = userns, run_as_uid = run_as_uid, run_as_gid = run_as_gid,
  })
  if not r.success then
    error("oci: container start failed: " .. (r.stderr or "unknown error"))
  end

  sessions[session.id] = { name = name, runtime = runtime, workspace = workspace }

  -- Default-deny: anything these handlers do not take over is refused rather
  -- than run on the host. The six intercepted tools were complete only by
  -- coincidence; a seventh would have escaped silently.
  crucible.require_isolation{
    session = session.id, plugin = "oci", exempt = cfg.exempt,
  }

  crucible.set_status{
    session = session.id, key = "oci", plugin = "oci",
    text = string.format("sandboxed: %s (%s)", cfg.image, runtime), level = "info",
  }
  cru.log("info", "oci: container started " .. name .. " (" .. cfg.image .. ")")
end, { required = true })

crucible.on_session_end(function(session)
  local active = sessions[session.id]
  if not active then return end
  container.stop(active.runtime, active.name)
  container.rm(active.runtime, active.name)
  sessions[session.id] = nil
  crucible.clear_status{ session = session.id, key = "oci" }
  cru.log("info", "oci: container removed " .. active.name)
end)

return {
  name = "oci",
  --- Receives the `[plugins.oci]` section at load.
  setup = function(cfg)
    config = cfg or {}
  end,
  version = "0.2.0",
  description = "Run agent tools inside OCI containers via generic hook interception",
}
