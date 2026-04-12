--- OCI container lifecycle operations
-- Thin wrappers around docker/podman CLI commands.
local M = {}

--- Create and start a container with the sleep infinity sidecar pattern.
function M.run(runtime, opts)
  local args = {
    "run", "-d",
    "--name", opts.name,
    "--label", "crucible=true",
    "--label", "crucible.session=" .. opts.session_id,
    "--security-opt", "no-new-privileges",
    "-w", "/workspace",
    "-v", opts.workspace .. ":/workspace:rw,z",
  }

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

  return cru.shell.exec(runtime, args, { timeout = 60 })
end

--- Build an image from a Dockerfile.
function M.build(runtime, opts)
  return cru.shell.exec(runtime, {
    "build", "-t", opts.image, "-f", opts.dockerfile, opts.context,
  }, { timeout = 300 })
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
