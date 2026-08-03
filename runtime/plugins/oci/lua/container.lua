--- OCI container lifecycle operations
-- Thin wrappers around docker/podman CLI commands.
local remap = require("remap")

local M = {}

--- Documented defaults for the two long operations.
---
--- An image pull on a cold host and a multi-stage build are the only calls here
--- that routinely outlast a default timeout, and both are configurable
--- (`start_timeout` / `build_timeout`). Note that `cru.shell.exec` buffers
--- output until the process exits, so a raised limit buys time, not progress.
M.DEFAULT_START_TIMEOUT = 300
M.DEFAULT_BUILD_TIMEOUT = 900

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
---
--- Probes with `cru.shell.which` — a PATH lookup, no subprocess. Running
--- `--version` was considered and proves little more: `docker --version`
--- succeeds with no daemon running, so a which-hit that can't actually serve
--- fails at container start with a real error either way.
function M.detect(configured)
  if configured and configured ~= "" then
    if cru.shell.which(configured) then
      return configured
    end
    return nil, string.format("configured runtime '%s' is not installed", configured)
  end

  for _, candidate in ipairs(M.CANDIDATES) do
    if cru.shell.which(candidate) then
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

--- The main repository's git directory, when it is OUTSIDE `workspace`.
---
--- A linked worktree's `.git` is not a directory but a file holding
--- `gitdir: <absolute host path>` into the main repo. Bind-mount the worktree
--- alone and that path does not exist in the container, so every git command
--- inside it fails with `not a git repository` — for the shape that is the
--- normal way to run parallel agents.
---
--- Returns nil when the workspace is an ordinary checkout (its common dir is
--- `<workspace>/.git`, already inside the mount) or is not a repository at all.
function M.git_common_dir(workspace)
  if not workspace or workspace == "" then return nil end
  local r = cru.shell.exec("git", {
    "-C", workspace, "rev-parse", "--path-format=absolute", "--git-common-dir",
  })
  if not (r and r.success) then return nil end
  local dir = (r.stdout or ""):gsub("%s+$", "")
  if dir == "" then return nil end
  -- Inside the workspace already: the ordinary checkout, nothing to add.
  if dir == workspace or dir:sub(1, #workspace + 1) == workspace .. "/" then
    return nil
  end
  return dir
end

--- The argv for `run`, as a pure function so tests can assert on it without
--- a runtime present. `run` is exactly `exec(runtime, run_args(opts, runtime))`.
---
--- `runtime` is passed as well as used as the command because one mount option
--- is podman-only; see the git-dir mount below.
function M.run_args(opts, runtime)
  -- The bind target and the working directory are the same value on purpose:
  -- mounting the workspace at one path and starting in another puts every
  -- relative tool call in an empty directory.
  local target = opts.target or remap.DEFAULT_TARGET
  local args = {
    "run", "-d",
    "--name", opts.name,
    "--label", "crucible=true",
    "--label", "crucible.session=" .. opts.session_id,
    "--security-opt", "no-new-privileges",
    "-w", target,
  }

  -- Uid mapping, before the bind-mount so it applies to it.
  --
  -- Both halves are required for a non-root image: `keep-id` alone leaves the
  -- process running as the image's own uid, which is not the mapped one, and
  -- writes still fail. See `image_runs_as_non_root` for the measurements.
  local pinned_uid
  if opts.userns and opts.userns ~= "" and opts.userns ~= false then
    table.insert(args, "--userns=" .. opts.userns)
    if opts.run_as_uid then
      pinned_uid = opts.run_as_uid .. ":" .. (opts.run_as_gid or opts.run_as_uid)
    end
  end

  -- `user` is a devcontainer's `remoteUser` — a user *in the image*. The pin is
  -- a numeric *host* id. When both apply the pin wins, because keep-id maps the
  -- host uid into the container and running as any other id fails every
  -- workspace write; the devcontainer CLI reaches the same place from the other
  -- side by renumbering remoteUser to the host uid (`updateRemoteUserUID`).
  local user = pinned_uid or opts.user
  if user then
    table.insert(args, "--user")
    table.insert(args, user)
  end

  table.insert(args, "-v")
  table.insert(args, opts.workspace .. ":" .. target .. ":rw,z")

  -- A linked worktree's `.git` points at the main repo by ABSOLUTE host path,
  -- so the same path has to exist in the container for git to resolve it.
  -- Mounted at its own path rather than remapped: the pointer is baked into
  -- files this must not rewrite, since they are the human's working tree too.
  -- rw because the agent commits — see the devcontainer trust note.
  --
  -- `--mount`, not `-v`, and that is not a style choice. Measured on podman
  -- 5.8.4: `-v <path>:<path>:rw,z` — source and destination IDENTICAL, which
  -- is exactly what this mount needs — is silently dropped. No warning, no
  -- failure, just a container whose git is broken. The same spec with two
  -- different paths mounts fine, and `--mount` with explicit keys mounts
  -- either way.
  if opts.git_common_dir then
    local spec = "type=bind,source=" .. opts.git_common_dir
      .. ",destination=" .. opts.git_common_dir
    -- SELinux denies the read without it (measured: `Permission denied` on
    -- .git/HEAD under Enforcing). Podman-only — docker and nerdctl reject the
    -- option, and take `:z` on `-v` instead.
    if (runtime or ""):match("podman$") then
      spec = spec .. ",relabel=shared"
    end
    table.insert(args, "--mount")
    table.insert(args, spec)
  end

  for _, m in ipairs(opts.mounts or {}) do
    table.insert(args, "-v")
    table.insert(args, m)
  end
  for k, v in pairs(opts.env or {}) do
    table.insert(args, "-e")
    table.insert(args, k .. "=" .. v)
  end

  -- A devcontainer's `runArgs` is raw runtime argv. It goes here, before the
  -- image: after it these would be arguments to the sidecar command instead.
  for _, arg in ipairs(opts.run_args or {}) do
    table.insert(args, arg)
  end

  table.insert(args, opts.image)
  table.insert(args, "sleep")
  table.insert(args, "infinity")
  return args
end

--- Create and start a container with the sleep infinity sidecar pattern.
--- Covers the image pull, which is the part that can take minutes.
function M.run(runtime, opts)
  return cru.shell.exec(runtime, M.run_args(opts, runtime), {
    timeout = opts.start_timeout or M.DEFAULT_START_TIMEOUT,
  })
end

--- Whether git resolves inside the container.
---
--- Returns `true`, or `false` plus a reason: `"no-git"` when the image ships no
--- git at all, `"unresolved"` when git is there but cannot find the repository.
---
--- Only meaningful for a workspace whose git dir lives outside it (a linked
--- worktree). The mount that makes it resolve is easy to lose silently — one
--- runtime drops a bind spec whose source and destination match, and SELinux
--- can deny the read even when the mount lands — and the symptom is a
--- container where every git command says "not a git repository". Checking
--- costs one exec at session start and turns that into a message.
---
--- The second exec is what separates the two. A slim base image with no git
--- fails the same `rev-parse`, and reporting a mount failure there sends
--- someone debugging a mount that is fine; it only runs when the first fails.
function M.git_works(runtime, name, target)
  local r = cru.shell.exec(runtime, {
    "exec", "-w", target, name, "git", "rev-parse", "--git-dir",
  })
  if r and r.success then return true end
  local v = cru.shell.exec(runtime, { "exec", name, "git", "--version" })
  if not (v and v.success) then return false, "no-git" end
  return false, "unresolved"
end

--- Build an image from a Dockerfile.
---
--- `build_args` is a devcontainer's `build.args`. Dropping it would build with
--- the Dockerfile's own ARG defaults — a different image than the editor's,
--- with nothing to show for it.
function M.build(runtime, opts)
  local args = { "build", "-t", opts.image, "-f", opts.dockerfile }

  -- Sorted, so a rebuild of the same config produces the same argv.
  local names = {}
  for name in pairs(opts.build_args or {}) do names[#names + 1] = name end
  table.sort(names)
  for _, name in ipairs(names) do
    table.insert(args, "--build-arg")
    table.insert(args, name .. "=" .. tostring(opts.build_args[name]))
  end

  table.insert(args, opts.context)

  -- Streamed, not buffered. A build takes minutes and `exec` returns nothing
  -- until it is over, so the status slot said "building alpine" and then went
  -- silent — indistinguishable from a hang. `on_progress` receives each line
  -- so the caller can report what the builder is actually doing.
  if opts.on_progress then
    return cru.shell.spawn(runtime, args, {
      timeout = opts.build_timeout or M.DEFAULT_BUILD_TIMEOUT,
      on_line = function(_, line) opts.on_progress(line) end,
    })
  end
  return cru.shell.exec(runtime, args, {
    timeout = opts.build_timeout or M.DEFAULT_BUILD_TIMEOUT,
  })
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
