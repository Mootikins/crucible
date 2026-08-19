--- OCI Container Plugin
-- Manages container lifecycle and tool interception for sandboxed workspace tool execution.
-- When a project has [container] config, this plugin:
-- 1. Creates a container on session start (sleep infinity sidecar pattern)
-- 2. Registers pre_tool_call handlers that intercept workspace tools (bash, read_file, etc.)
-- 3. Routes intercepted tool calls through `docker exec` inside the container
-- 4. Cleans up the container on session end
--
-- The daemon knows nothing about containers — every container decision is made
-- here, through generic crucible.on() hooks with pattern matching and the
-- Handled result convention.

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

--- Claim isolation for a session, including how to run a command inside its
--- container.
---
--- The exec argv is handed over so an ACP agent can be launched *into* the
--- sandbox instead of beside it. The daemon cannot intercept tools an external
--- agent runs in its own process — so a claimed session with an external agent
--- is refused — but it does not have to when that process is already in the
--- container. This is what lifts the refusal.
---
--- `-i` because the agent speaks JSON-RPC over stdin/stdout, and `-w` so it
--- starts where its own tools expect the workspace to be.
---
--- Given in two halves because the daemon inserts the agent's configured
--- environment between them: every runtime takes a command's flags *before*
--- its container operand, so `exec crucible-x -e KEY=v agent` would pass the
--- flag to the agent instead. Naming the flag here keeps that knowledge on
--- this side — the daemon only fills the hole, and never learns that the
--- operand is a container.
local function claim_isolation(session_id, exempt, runtime, name, target)
  crucible.require_isolation{
    session = session_id, plugin = "oci", exempt = exempt,
    exec_prefix = { runtime, "exec", "-i", "-w", target },
    exec_env_flag = "-e",
    exec_suffix = { name },
  }
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
  local w = cru.shell.exec(active.runtime, {
    "exec", "-i", active.name, "sh", "-c", write_script,
  }, { stdin = new_content })

  -- Checked, like the read above it. Reporting a replacement that a read-only
  -- mount or a full disk rejected tells the model its edit landed, and the next
  -- thing it does is built on a file that never changed.
  if not w.success then
    return { handled = true, result = cru.json.encode({
      result = "Error: " .. (w.stderr or "write failed")
    }) }
  end

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
  -- Operator config, deliberately: it lives outside the workspace, so it is the
  -- one input the sandboxed agent cannot write. See devcontainer.HOST_KEYS.
  return devcontainer.resolve(session and session.workspace,
    config.devcontainer_host_access == true)
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
    -- An ADDRESSED target: `{ plugin = "ssh", target = "build-box" }`, the
    -- shape the runtime chip sends now that more than one plugin answers on
    -- this channel. One addressed elsewhere is not this plugin's business, and
    -- must not raise — before the axes were separated, the only thing this
    -- plugin could do with a name it did not recognise was fail the session,
    -- which made a second runtime provider impossible.
    if requested.plugin ~= nil then
      if requested.plugin ~= "oci" then return nil end
      local target = requested.target
      if target == nil or target == "" then
        -- Addressed here, naming nothing: the default profile, resolved below
        -- exactly as a bare `true` is.
        requested = true
      else
        local profile = config.profiles and config.profiles[target]
        if not profile then
          error("oci: unknown isolation profile '" .. tostring(target) .. "'")
        end
        return environment(profile)
      end
    else
      if not requested.image then
        error("oci: the session's isolation object names no image")
      end
      return environment(requested)
    end
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
    claim_isolation(session.id, cfg.exempt, shared.runtime, shared.name, shared.target)
    crucible.set_status{
      session = session.id, key = "oci", plugin = "oci",
      text = string.format("sandboxed: %s (%s)", cfg.image, shared.runtime), level = "info",
    }
    cru.log("info", "oci: session " .. session.id .. " joined container " .. shared.name)
    return
  end

  -- Set only when the workspace's git dir lives outside it — a linked worktree,
  -- where git's own pointer into the main repo is an absolute host path.
  --
  -- Resolved before the branch because both paths need it: the plugin's own run
  -- mounts it, the CLI has to be told about it, and the check at the end has to
  -- run either way. It used to be resolved inside the else branch only, so a
  -- devcontainer built by the CLI on a worktree got no mount and no warning.
  local git_dir = container.git_common_dir(workspace)

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
    local up_args = { "up", "--workspace-folder", workspace }
    -- The CLI owns the container's mounts, so a worktree's main repo can only
    -- reach it as an argument. Its `--mount` spells the destination `target=`
    -- (podman's own is `destination=`) and takes no relabel key, so under
    -- SELinux the mount can land and still be denied — which is exactly what
    -- the git check at the end of this hook reports.
    if git_dir then
      table.insert(up_args, "--mount")
      table.insert(up_args, "type=bind,source=" .. git_dir .. ",target=" .. git_dir)
    end
    local up = cru.shell.exec(devcontainer.CLI, up_args,
      { timeout = cfg.build_timeout or container.DEFAULT_BUILD_TIMEOUT })

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
      -- Only set for a linked worktree (or a workspace below the repo root),
      -- where git's own pointer into the main repo is an absolute host path.
      git_common_dir = git_dir,
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
  claim_isolation(session.id, cfg.exempt, runtime, name, cfg.target)

  local text = string.format("sandboxed: %s (%s)", cfg.image, runtime)
  local level = "info"

  -- Said out loud rather than left to be discovered mid-turn. A worktree whose
  -- main repo did not mount gives a container where every git command fails
  -- with "not a git repository", and the agent has no way to tell that from a
  -- project that simply is not version controlled.
  --
  -- Which of the two failures it is decides where to look, so the message says:
  -- a base image with no git is an image choice, a lost mount is a mount.
  if git_dir then
    local git_ok, why = container.git_works(runtime, name, cfg.target)
    if not git_ok then
      text = text .. (why == "no-git"
        and " — the image has no git, so this worktree's repository is unusable"
        or " — git cannot resolve the repository in the container (worktree mount failed)")
      level = "warn"
    end
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

local plugin = {
  name = "oci",

  commands = {
    --- The container environments this box offers, as menu rows.
    ---
    --- A command rather than published data so the answer tracks a reloaded
    --- config and, for the devcontainer row, the project actually selected —
    --- which the publication, made once at load, cannot know.
    ["oci.targets"] = {
      desc = "Container environments available as runtime targets",
      fn = function(args)
        local targets = {}

        -- ONE unnamed row, not one per possible source. The devcontainer and
        -- the configured image are not a choice the user makes — they are the
        -- same request (`isolation = true`), and `resolve_config` decides
        -- between them by precedence. Offering both would present a pick that
        -- does not exist, and both would carry the same empty value anyway.
        -- The hint says which one it will actually be, for this project.
        local workspace = args and args.workspace
        local has_devcontainer = workspace ~= nil
          and config.devcontainer ~= false
          and devcontainer.find(workspace) ~= nil
        if has_devcontainer or config.image then
          targets[#targets + 1] = {
            value = "",
            label = "Default",
            hint = has_devcontainer and "this project's devcontainer" or config.image,
          }
        end

        local names = {}
        for name in pairs(config.profiles or {}) do names[#names + 1] = name end
        table.sort(names)
        for _, name in ipairs(names) do
          local profile = config.profiles[name]
          targets[#targets + 1] = {
            value = name,
            label = name,
            hint = type(profile) == "table" and profile.image or nil,
          }
        end

        return { targets = targets }
      end,
    },
  },

  --- Receives the `[plugins.oci]` section at load.
  setup = function(cfg)
    config = cfg or {}

    -- Declared once; the TUI and the web settings pane render it in their own
    -- idiom. `values` and `desc` are functions where the answer depends on
    -- this box rather than on this file — the reason the tree is read live
    -- instead of converted at load.
    crucible.options{
      type = "group",
      name = "Container isolation",
      get = function(info) return config[info.option] end,
      set = function(info, value) config[info.option] = value end,
      args = {
        image = {
          type = "input", order = 1, name = "Default image",
          desc = "Image workspace tools run in when a project has no devcontainer.",
        },
        runtime = {
          type = "select", order = 2, name = "Container runtime",
          desc = "Left unset, the first of podman, docker, nerdctl on PATH is used.",
          -- Only what is actually installed here, resolved when the settings
          -- are opened rather than when this file loaded.
          values = function()
            local found = {}
            for _, name in ipairs(container.CANDIDATES) do
              if cru.shell.which(name) then found[#found + 1] = name end
            end
            return found
          end,
        },
        devcontainer = {
          type = "toggle", order = 3, name = "Use the project's devcontainer",
          desc = "Read .devcontainer/devcontainer.json when the project has one. "
            .. "Only the committed file is honoured.",
        },
        devcontainer_host_access = {
          type = "toggle", order = 4, name = "Let a devcontainer reach the host",
          desc = "Honour the keys a devcontainer may not otherwise ask for — runArgs, "
            .. "mounts, initializeCommand, features and compose. Off by default: this "
            .. "session's agent can write that file, and those keys configure the "
            .. "container from outside it. Applies to every project, not just this one.",
        },
        build_timeout = {
          type = "range", order = 5, name = "Build timeout (seconds)",
          min = 60, max = 3600, step = 60,
        },
        start_timeout = {
          type = "range", order = 6, name = "Start timeout (seconds)",
          min = 30, max = 1800, step = 30,
        },
        cleanup = {
          type = "execute", order = -1, name = "Remove orphaned containers",
          desc = "Remove crucible containers whose session is gone.",
          func = function()
            local runtime = container.detect(config.runtime)
            if runtime then cleanup_orphans(runtime) end
          end,
        },
      },
    }

    -- The runtime axis: *where does the process run?* Declared as a provider
    -- rather than as an isolation offer, because `ssh` answers the same
    -- question and the old shape had no way to say who was answering. The
    -- workspace axis — which worktree, which checkout — is a separate provider
    -- in a separate plugin, and the two compose.
    --
    -- Published only when this box actually offers containers. A provider that
    -- publishes nothing contributes no menu entry, which is how "unavailable"
    -- is now said; the `available` flag existed because the old channel had no
    -- way not to answer.
    if config.image ~= nil or config.profiles ~= nil or config.devcontainer == true then
      crucible.publish("targets", {
        axis = "runtime",
        label = "Container",
        targets_command = "oci.targets",
      })
    end
  end,
  version = "0.2.0",
  description = "Run agent tools inside OCI containers via generic hook interception",
}

-- The daemon executes this file BY PATH, not through `require`, so nothing
-- would otherwise fill `package.loaded`. The documented
-- `require("oci").setup{...}` from a user's init.lua would then load a
-- SECOND copy of this file: with its own upvalues, and re-running every
-- body-level `crucible.on_*` call in it — so the `on_session_start` and `on_session_end` handlers at :496 and :701 would be
-- registered twice and fire twice per event. Registering the spec here makes
-- that `require` answer with this table instead.
--
-- Same guard and same reason as `auto-title` and `web-search`. Enumerated
-- across every bundled plugin on 2026-08-19 after the auto-title fix stopped
-- at one plugin; `plugin_require_guard` pins the whole set.
package.loaded["oci"] = plugin

return plugin
