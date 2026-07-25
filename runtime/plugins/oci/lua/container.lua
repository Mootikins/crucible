--- OCI container lifecycle operations
-- Thin wrappers around docker/podman CLI commands.
local M = {}

--- Runtimes probed in order when none is configured.
---
--- podman first: it is rootless by default, which is the safer posture and
--- increasingly the distro default. This is a preference order, not a
--- capability claim — all three accept the subcommands used here.
M.CANDIDATES = { "podman", "docker", "nerdctl" }

--- Find a usable container runtime.
---
--- Returns `runtime`, or `nil, reason`. The reason names everything tried:
--- "no container runtime" is unactionable, "tried podman, docker, nerdctl" is.
function M.detect(configured)
  if configured and configured ~= "" then
    local probe = cru.shell.exec(configured, { "--version" })
    if probe and probe.success then
      return configured
    end
    return nil, string.format("configured runtime '%s' is not usable", configured)
  end

  for _, candidate in ipairs(M.CANDIDATES) do
    local probe = cru.shell.exec(candidate, { "--version" })
    if probe and probe.success then
      return candidate
    end
  end
  return nil, "no container runtime found (tried " .. table.concat(M.CANDIDATES, ", ") .. ")"
end

--- Whether the image's configured USER is non-root.
---
--- Decides whether uid mapping is needed. Measured against rootless podman
--- 5.8.4 with a host uid of 1000, writing into a bind-mounted workspace:
---
---   root image (`Config.User` empty), no flags     -> host file owned by 1000  OK
---   USER appuser(1500), no flags                   -> EACCES
---   USER appuser(1500), `--userns=keep-id`         -> EACCES  (still fails!)
---   USER appuser(1500), keep-id + `--user 1000:1000` -> owned by 1000  OK
---
--- The third line is the trap: bare `keep-id` maps the *host* user into the
--- container as the same uid, but the image still runs its process as 1500,
--- which is not the mapped id — so writes fail exactly as they did without it.
--- Mapping only works when the running uid is also pinned. Do not "simplify"
--- this to a bare keep-id; it was tried and it does not work.
function M.image_runs_as_non_root(runtime, image)
  local r = cru.shell.exec(runtime, {
    "image", "inspect", "--format", "{{.Config.User}}", image,
  })
  if not r or not r.success then
    -- Unknown: assume non-root and map. Wrong here costs a re-chown; wrong
    -- the other way makes every write in the container fail.
    return true
  end
  local user = (r.stdout or ""):gsub("%s+", "")
  return user ~= "" and user ~= "root" and user ~= "0"
end

--- The invoking user's uid/gid on the host.
---
--- Needed to pin the container's running uid when mapping a non-root image;
--- `keep-id` alone is not enough. Read via `id` rather than a Lua API because
--- none exposes it, and `id` is present anywhere a container runtime is.
function M.host_ids()
  local u = cru.shell.exec("id", { "-u" })
  local g = cru.shell.exec("id", { "-g" })
  if not u or not u.success then return nil end
  local uid = (u.stdout or ""):gsub("%s+", "")
  local gid = (g and g.success) and (g.stdout or ""):gsub("%s+", "") or uid
  if uid == "" then return nil end
  return uid, gid
end

--- Create and start a container with the sleep infinity sidecar pattern.
function M.run(runtime, opts)
  local args = {
    "run", "-d",
    "--name", opts.name,
    "--label", "crucible=true",
    "--label", "crucible.session=" .. opts.session_id,
    "--security-opt", "no-new-privileges",
    "-w", "/workspace",
  }

  -- Uid mapping, before the bind-mount so it applies to it.
  --
  -- Both halves are required for a non-root image: `keep-id` alone leaves the
  -- process running as the image's own uid, which is not the mapped one, and
  -- writes still fail. See `image_runs_as_non_root` for the measurements.
  if opts.userns and opts.userns ~= "" and opts.userns ~= false then
    table.insert(args, "--userns=" .. opts.userns)
    if opts.run_as_uid then
      table.insert(args, "--user")
      table.insert(args, opts.run_as_uid .. ":" .. (opts.run_as_gid or opts.run_as_uid))
    end
  end
  table.insert(args, "-v")
  table.insert(args, opts.workspace .. ":/workspace:rw,z")

  for _, m in ipairs(opts.mounts or {}) do
    table.insert(args, "-v")
    table.insert(args, m)
  end
  for k, v in pairs(opts.env or {}) do
    table.insert(args, "-e")
    table.insert(args, k .. "=" .. v)
  end

  table.insert(args, opts.image)
  table.insert(args, "sleep")
  table.insert(args, "infinity")

  return cru.shell.exec(runtime, args, { timeout = 300 })
end

--- Build an image from a Dockerfile.
function M.build(runtime, opts)
  return cru.shell.exec(runtime, {
    "build", "-t", opts.image, "-f", opts.dockerfile, opts.context,
  }, { timeout = 900 })
end

--- Stop a container (5 second grace period).
function M.stop(runtime, name)
  return cru.shell.exec(runtime, { "stop", "-t", "5", name })
end

--- Force-remove a container.
function M.rm(runtime, name)
  return cru.shell.exec(runtime, { "rm", "-f", name })
end

--- Check if a container is currently running.
function M.is_running(runtime, name)
  local r = cru.shell.exec(runtime, {
    "inspect", "--format", "{{.State.Running}}", name,
  })
  return r.success and r.stdout:match("true") ~= nil
end

--- List all crucible-labeled containers.
function M.list_crucible(runtime)
  local r = cru.shell.exec(runtime, {
    "ps", "-a",
    "--filter", "label=crucible=true",
    "--format", "{{.Names}}\t{{.Label \"crucible.session\"}}\t{{.Status}}",
  })
  if not r.success then return {} end

  local containers = {}
  for line in r.stdout:gmatch("[^\n]+") do
    local name, sid, status = line:match("^(.-)\t(.-)\t(.+)$")
    if name then
      table.insert(containers, { name = name, session_id = sid, status = status })
    end
  end
  return containers
end

return M
