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
local devcontainer = require("devcontainer")
local remap = require("remap")

-- One container per distinct WORKSPACE, not per session.
--
-- A delegated child inherits its parent's workspace, so both would bind-mount
-- the same directory — a second container is a cold start that buys nothing,
-- and the subagent case then needs no inherit/fresh config flag at all. When a
-- child gets its own worktree it has a distinct workspace and therefore its own
-- container, by the same rule and with no branch added anywhere.
--
-- `refs` is why a parent's container outlives its children: teardown removes it
-- only when the last session using it ends.
local containers = {} -- workspace -> { name, runtime, workspace, image, target, refs }

-- Per session, because handlers are registered once at plugin load and are
-- shared by every session. A single global was only ever correct when
-- handlers were (re-)registered per session — which also made them
-- accumulate, one stale copy per session, for the daemon's lifetime.
local sessions = {} -- session_id -> workspace

--- Count `session_id` against `workspace`'s container, exactly once.
---
--- `on_session_start` fires on create, resume AND resume_from_storage — and a
--- web history fetch calls resume_from_storage on every request — while
--- `on_session_end` fires once per session. Counting each *call* leaked a
--- reference per fetch, so the container was never removed for the daemon's
--- lifetime. Counting each *session* is what the refcount always meant.
---
--- Registration is by workspace, not by mere presence in `sessions`: the same
--- session id can legitimately be re-registered, and only a repeat of the same
--- workspace is already counted. A session that moves releases the container it
--- left, so a re-fire cannot strand a reference on the old one either.
local function register(session_id, workspace)
  local previous = sessions[session_id]
  if previous == workspace then return end
  if previous and containers[previous] then
    containers[previous].refs = containers[previous].refs - 1
  end
  local shared = containers[workspace]
  if shared then shared.refs = shared.refs + 1 end
  sessions[session_id] = workspace
end

--- The container serving this tool call, or nil if the session has none.
local function active_for(ctx)
  local id = ctx and ctx.session_id
  if not id then return nil end
  local workspace = sessions[id]
  if not workspace then return nil end
  return containers[workspace]
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
    "exec", "-w", active.target, active.name, "sh", "-c", cmd,
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
  local path = remap_path(active.workspace, args.path, active.target)
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
  local path = remap_path(active.workspace, args.path, active.target)
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
  local path = remap_path(active.workspace, args.path, active.target)
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

  local search_dir = args.path and remap_path(active.workspace, args.path, active.target) or active.target

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

  local search_path = args.path and remap_path(active.workspace, args.path, active.target) or active.target

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
--
-- Empty, not nil: setup() is not called when there is no [plugins.oci] section
-- at all, and resolution reads sub-keys unconditionally.
local config = {}

--- Normalise one environment definition — the bare `[plugins.oci]` section, a
--- named profile, or an inline session override — into what the start hook uses.
---
--- `runtime` and `exempt` fall back to the top level because they describe the
--- box and the session policy, not the image: a profile that omitted `runtime`
--- would otherwise silently re-detect and ignore a deliberately configured one.
--- `target` is where the workspace is mounted *inside* the container. It is one
--- value because six things have to agree on it: the bind mount, the working
--- directory, the paths tools are handed, and the glob and grep search roots.
--- /workspace unless something says otherwise — a devcontainer's
--- `workspaceFolder` is typically /workspaces/<name>.
---
--- The timeouts fall back to the top level for the same reason `runtime` does:
--- they describe how slow this box is, not what the image is.
local function environment(p)
  return {
    image = p.image,
    runtime = p.runtime or config.runtime,
    dockerfile = p.dockerfile,
    build_context = p.build_context,
    build_args = p.build_args,
    mounts = p.mounts or {},
    env = p.env or {},
    run_args = p.run_args or {},
    user = p.user,
    userns = p.userns,
    cli = p.cli,
    target = p.workspace_folder or config.workspace_folder or remap.DEFAULT_TARGET,
    build_timeout = p.build_timeout or config.build_timeout,
    start_timeout = p.start_timeout or config.start_timeout,
    exempt = p.exempt or config.exempt or {},
    -- Devcontainer-only: set when the committed file that was honoured differs
    -- from the one in the working tree. Nil for profiles, which have no
    -- committed form to diverge from.
    uncommitted_drift = p.uncommitted_drift,
  }
end

--- The project's `.devcontainer/devcontainer.json`, when isolation was asked
--- for at all.
---
--- Gated deliberately. The plugin ships enabled in every install, so reading a
--- devcontainer unconditionally would containerize any repo that happens to
--- carry one — and, on a box with no container runtime, refuse every session in
--- it. Isolation stays opt-in; the devcontainer decides *what* to build once
--- something has asked for isolation at all. `devcontainer = false` opts a
--- project back out, so a repo with both a devcontainer and a profile can
--- choose the profile.
local function project_devcontainer(session, requested)
  if config.devcontainer == false then return nil end
  local asked = requested == true
    or config.devcontainer == true
    or config.image ~= nil
    or config.profiles ~= nil
  if not asked then return nil end
  return devcontainer.resolve(session and session.workspace)
end

--- Resolve the environment for a session, first hit wins:
---
---   1. the session's own `isolation` param
---   2. the project's `.devcontainer/devcontainer.json`
---   3. a named `[plugins.oci.profiles]` entry, or the bare `image` key
---      (the implicit default profile)
---   4. none — no container
---
--- The devcontainer outranks the profile because it is the project's own
--- statement of what the environment is: an agent working in a container that
--- differs from the one the human's editor builds is a subtler version of the
--- same failure as an agent that is not sandboxed at all.
---
--- `session.isolation` is what `session.create` was handed, forwarded by the
--- daemon untouched. `nil` means the caller said nothing; `false` means "no
--- container even if the project has one" and is the one value that can turn
--- isolation *off*. Anything the plugin cannot honour raises rather than
--- falling back: isolation asked for and not delivered is never a silently
--- unsandboxed session.
local function resolve_config(session)
  local requested = session and session.isolation
  if requested == false then return nil end

  if type(requested) == "string" then
    local profile = config.profiles and config.profiles[requested]
    if not profile then
      error("oci: unknown isolation profile '" .. requested .. "'")
    end
    return environment(profile)
  end

  if type(requested) == "table" then
    if not requested.image then
      error("oci: the session's isolation object names no image")
    end
    return environment(requested)
  end

  if requested ~= nil and requested ~= true then
    error("oci: isolation must be false, true, a profile name, or a table; got "
      .. type(requested))
  end

  local dc = project_devcontainer(session, requested)
  if dc then return environment(dc) end

  -- `true` asks for isolation without saying which — the default profile, and
  -- a refusal rather than a no-op when there is no default to resolve.
  if requested == true and not config.image then
    error("oci: this session asked to be isolated, but no .devcontainer/devcontainer.json, "
      .. "[plugins.oci] image or profile is configured")
  end

  if config.devcontainer == true and not config.image then
    error("oci: [plugins.oci] sets devcontainer = true, but this workspace has no "
      .. ".devcontainer/devcontainer.json and no image is configured to fall back to")
  end

  if not config.image then return nil end
  return environment(config)
end

-- ─────────────────────────────────────────────────────────────────────────────
-- Orphan cleanup
-- ─────────────────────────────────────────────────────────────────────────────

--- Names this plugin is still serving sessions from.
---
--- A shared container is named after whichever session created it, and that
--- session can end while a delegated child is still using it. Without this the
--- next session's orphan sweep would remove a container in active use, because
--- its label names a session that is gone.
local function in_use()
  local names = {}
  for _, active in pairs(containers) do
    names[active.name] = true
  end
  return names
end

local function cleanup_orphans(runtime)
  if not runtime then return end
  local live = in_use()
  for _, c in ipairs(container.list_crucible(runtime)) do
    local session = cru.sessions and cru.sessions.get(c.session_id)
    if not session and not live[c.name] then
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
  local cfg = resolve_config(session)
  -- Not configured is not a failure: no [plugins.oci] image means the user
  -- never asked for isolation, and every session would otherwise be refused.
  -- `isolation = false` lands here too — a session that declined the project's
  -- container is not a session whose sandbox broke.
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

  -- One container per workspace: a delegated child sharing its parent's
  -- workspace joins the parent's container instead of paying a cold start for
  -- an identical bind mount.
  local shared = containers[workspace]
  if shared then
    -- Verify, don't assume. If the container died, quietly starting a fresh
    -- one would leave the sessions already registered against it running
    -- against nothing — and silently starting one for this session while the
    -- others are broken is exactly the "sandbox broken looks like sandbox
    -- working" state the required hook exists to prevent.
    if not container.is_running(shared.runtime, shared.name) then
      error("oci: the container for this workspace (" .. shared.name ..
        ") is no longer running; refusing the session rather than running it on the host")
    end
    -- One container per workspace means a session asking for a *different*
    -- environment on that workspace cannot have it. Joining anyway would give
    -- it an image it did not ask for, which is the environment equivalent of a
    -- silent downgrade.
    if shared.image ~= cfg.image then
      error("oci: this session asked for " .. cfg.image .. " but its workspace is " ..
        "already sandboxed with " .. shared.image ..
        "; one container per workspace, so the environments must match")
    end
    -- Same rule for where the workspace lands inside it: joining would run this
    -- session's tools against a mount target it did not ask for, and the mount
    -- cannot be moved without recreating the container.
    if shared.target ~= cfg.target then
      error("oci: this session asked for its workspace at " .. cfg.target ..
        " but its workspace is already sandboxed with the mount at " .. shared.target ..
        "; one container per workspace, so the environments must match")
    end
    register(session.id, workspace)
    crucible.require_isolation{
      session = session.id, plugin = "oci", exempt = cfg.exempt,
    }
    crucible.set_status{
      session = session.id, key = "oci", plugin = "oci",
      text = string.format("sandboxed: %s (%s)", cfg.image, shared.runtime), level = "info",
    }
    cru.log("info", "oci: session " .. session.id .. " joined container " .. shared.name)
    return
  end

  if cfg.cli then
    -- The devcontainer named something only @devcontainers/cli can build, and
    -- it is installed, so it builds the environment and this plugin adopts the
    -- container it produced. Adopting rather than reimplementing is the whole
    -- reason the CLI is reached for: `features` and the lifecycle commands are
    -- exactly the parts an approximation would get subtly wrong.
    crucible.set_status{
      session = session.id, key = "oci", plugin = "oci",
      text = "devcontainer up", level = "info",
    }
    local up = cru.shell.exec(devcontainer.CLI, {
      "up", "--workspace-folder", workspace,
    }, { timeout = cfg.build_timeout or container.DEFAULT_BUILD_TIMEOUT })

    local outcome = devcontainer.up_result(up.stdout)
    if not up.success or not outcome or outcome.outcome ~= "success" then
      error("oci: devcontainer up failed: "
        .. ((outcome and outcome.message) or up.stderr or "unknown error"))
    end
    name = outcome.containerId
    -- The CLI decides where it mounted the workspace. Every remapped path, the
    -- working directory and the search roots have to agree with that, not with
    -- what we would have chosen.
    cfg.target = outcome.remoteWorkspaceFolder or cfg.target
  else
    if cfg.dockerfile and cfg.dockerfile ~= "" then
      crucible.set_status{
        session = session.id, key = "oci", plugin = "oci",
        text = "building " .. cfg.image, level = "info",
      }
      local b = container.build(runtime, {
        -- The builder's own output is the only honest progress signal here:
        -- there is no total to count against, so the slot reports what step it
        -- is on and spins rather than inventing a fraction.
        on_progress = function(line)
          crucible.set_status{
            session = session.id, key = "oci", plugin = "oci",
            text = "building " .. cfg.image .. ": " .. line,
            level = "info", progress = true,
          }
        end,
        image = cfg.image, dockerfile = cfg.dockerfile,
        -- A devcontainer's build context is the .devcontainer directory, not
        -- the workspace root; a profile's Dockerfile has always built from the
        -- workspace.
        context = cfg.build_context or workspace,
        build_args = cfg.build_args,
        build_timeout = cfg.build_timeout,
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
      image = cfg.image, mounts = cfg.mounts, env = cfg.env, target = cfg.target,
      run_args = cfg.run_args, user = cfg.user,
      userns = userns, run_as_uid = run_as_uid, run_as_gid = run_as_gid,
      start_timeout = cfg.start_timeout,
    })
    if not r.success then
      error("oci: container start failed: " .. (r.stderr or "unknown error"))
    end
  end

  -- `target` rides on the container, not the session: it is a property of the
  -- mount, and every tool call resolves it back through `active_for`.
  containers[workspace] = {
    name = name, runtime = runtime, workspace = workspace, image = cfg.image,
    target = cfg.target, refs = 0,
  }
  register(session.id, workspace)

  -- Default-deny: anything these handlers do not take over is refused rather
  -- than run on the host. The six intercepted tools were complete only by
  -- coincidence; a seventh would have escaped silently.
  crucible.require_isolation{
    session = session.id, plugin = "oci", exempt = cfg.exempt,
  }

  -- A devcontainer is resolved from HEAD, so an edit sitting in the working
  -- tree changed nothing. Saying so beats letting someone conclude the plugin
  -- ignored their config.
  local text = string.format("sandboxed: %s (%s)", cfg.image, runtime)
  local level = "info"
  if cfg.uncommitted_drift then
    text = text .. " — uncommitted devcontainer edits are not applied"
    level = "warn"
  end

  crucible.set_status{
    session = session.id, key = "oci", plugin = "oci", text = text, level = level,
  }
  cru.log("info", "oci: container started " .. name .. " (" .. cfg.image .. ")")
end, { required = true })

crucible.on_session_end(function(session)
  local workspace = sessions[session.id]
  if not workspace then return end
  sessions[session.id] = nil
  crucible.clear_status{ session = session.id, key = "oci" }

  local active = containers[workspace]
  if not active then return end
  -- Refcount, so a parent's container outlives the delegated children sharing
  -- it. Removing on the first end would pull the sandbox out from under every
  -- other session on the same workspace — and a session whose container is
  -- gone is a session running on the host.
  active.refs = active.refs - 1
  if active.refs > 0 then
    cru.log("info", string.format("oci: container %s still in use by %d session(s)",
      active.name, active.refs))
    return
  end

  containers[workspace] = nil
  container.stop(active.runtime, active.name)
  container.rm(active.runtime, active.name)
  cru.log("info", "oci: container removed " .. active.name)
end)

return {
  name = "oci",
  --- Receives the `[plugins.oci]` section at load.
  setup = function(cfg)
    config = cfg or {}

    -- Tell clients what this box offers, so none of them has to read this
    -- plugin's config to work it out. `crucible-web` used to match on the
    -- shape of `[plugins.oci]` — a `profiles` table meant profiles existed —
    -- which put this schema in the rendering layer, missed the documented bare
    -- `image` config entirely, and would have ignored a second isolating
    -- plugin. Only the plugin knows what its own config means.
    --
    -- `available` and the profile *names* are the whole contract. What a
    -- profile resolves to is server-side detail and stays here.
    local names = {}
    for name in pairs(config.profiles or {}) do names[#names + 1] = name end
    table.sort(names)

    crucible.publish("isolation", {
      available = config.image ~= nil
        or config.profiles ~= nil
        or config.devcontainer == true,
      profiles = names,
    })
  end,
  version = "0.2.0",
  description = "Run agent tools inside OCI containers via generic hook interception",
}
