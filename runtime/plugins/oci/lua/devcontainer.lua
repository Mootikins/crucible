--- Read a project's `.devcontainer/devcontainer.json` into the environment
--- shape the rest of the plugin uses.
---
--- The rule this module exists to enforce: **an environment asked for and not
--- delivered is never silently replaced with one we happened to understand.**
--- A devcontainer names what the editor would build; anything in it this plugin
--- cannot reproduce either goes to `@devcontainers/cli`, which can, or refuses
--- the session naming the key. Quietly dropping a key would hand the agent a
--- different machine than the human works in, which is the environment
--- equivalent of a sandbox that silently did not start.
---
--- Everything but `resolve` is pure, so the parsing and the resolution order
--- are testable with no container runtime and no devcontainer CLI installed.
local M = {}

--- Where a devcontainer lives, in the order the spec calls canonical.
M.CANDIDATES = { ".devcontainer/devcontainer.json", ".devcontainer.json" }

--- The binary that honours what this plugin cannot.
M.CLI = "devcontainer"

--- Keys with no native equivalent. `@devcontainers/cli` builds these; without
--- it the session is refused naming them.
---
--- `features` and `dockerComposeFile` change what the image *is*;
--- the lifecycle commands change what is *in* it. Approximating either is how
--- an agent ends up in a container missing the toolchain the project assumes.
M.CLI_KEYS = {
  features = true,
  dockerComposeFile = true,
  service = true,
  runServices = true,
  initializeCommand = true,
  onCreateCommand = true,
  updateContentCommand = true,
  postCreateCommand = true,
  postStartCommand = true,
  postAttachCommand = true,
}

--- Keys that describe the *editor*, not the container.
---
--- This list is short and stays short: it is the only place a key is dropped
--- rather than honoured or refused, so every entry has to be one that cannot
--- change what a headless agent's container is. Port forwarding qualifies
--- because it exists to reach a container *from an editor on the host*, and
--- there is no such consumer here — a container that genuinely needs a
--- published port says so in `runArgs`, which is honoured.
M.IGNORED = {
  ["$schema"] = true,
  name = true,
  customizations = true,
  forwardPorts = true,
  portsAttributes = true,
  otherPortsAttributes = true,
}

--- `build` sub-keys with a native equivalent. Anything else under `build` is
--- refused for the same reason a top-level unknown is.
local BUILD_KEYS = { dockerfile = true, context = true, args = true }

--- Mount attributes, mapped to the two things a `-v` spec carries.
--- `consistency` is a macOS filesystem-performance hint with no effect here.
local MOUNT_KEYS = {
  type = "type",
  source = "source",
  src = "source",
  target = "target",
  destination = "target",
  dst = "target",
  readonly = "readonly",
  consistency = "consistency",
}

-- ─────────────────────────────────────────────────────────────────────────────
-- Paths
-- ─────────────────────────────────────────────────────────────────────────────

local function basename(path)
  return (path or ""):match("([^/]+)/?$") or ""
end

local function dirname(path)
  return path:match("^(.*)/[^/]+/?$") or "."
end

--- Resolve a devcontainer-relative path against the directory holding the file.
---
--- `build.dockerfile` and `build.context` are relative to the *devcontainer
--- directory*, not the workspace root. Resolving them against the workspace
--- builds from a Dockerfile that is usually not there — and when it is, it is
--- the wrong one.
local function resolve_rel(dir, path)
  if path:sub(1, 1) == "/" then return path end
  -- `"context": "."` is the spec's own default and the common way to write it;
  -- `<dir>/.` is the same directory to a runtime but not to a reader, or to a
  -- test asserting on the argv.
  if path == "." or path == "./" then return dir end
  return dir .. "/" .. (path:gsub("^%./", ""))
end

-- ─────────────────────────────────────────────────────────────────────────────
-- JSONC
-- ─────────────────────────────────────────────────────────────────────────────

--- Copy the string literal starting at `i` into `out`; returns the next index.
---
--- Both stripping passes need this: a `//` inside a URL and a `,` inside a
--- message are not syntax, and a pass that cannot tell the difference corrupts
--- the file it is trying to read.
local function copy_string(text, i, out)
  out[#out + 1] = text:sub(i, i)
  i = i + 1
  local n = #text
  while i <= n do
    local c = text:sub(i, i)
    if c == "\\" then
      out[#out + 1] = text:sub(i, i + 1)
      i = i + 2
    else
      out[#out + 1] = c
      i = i + 1
      if c == '"' then return i end
    end
  end
  return i
end

local function strip_comments(text)
  local out, i, n = {}, 1, #text
  while i <= n do
    local c = text:sub(i, i)
    if c == '"' then
      i = copy_string(text, i, out)
    elseif c == "/" and text:sub(i + 1, i + 1) == "/" then
      i = text:find("\n", i, true) or (n + 1)
    elseif c == "/" and text:sub(i + 1, i + 1) == "*" then
      local close = text:find("*/", i + 2, true)
      i = close and (close + 2) or (n + 1)
    else
      out[#out + 1] = c
      i = i + 1
    end
  end
  return table.concat(out)
end

local function strip_trailing_commas(text)
  local out, i, n = {}, 1, #text
  while i <= n do
    local c = text:sub(i, i)
    if c == '"' then
      i = copy_string(text, i, out)
    elseif c == "," then
      local j = text:find("[^%s]", i + 1)
      local next_char = j and text:sub(j, j)
      if next_char == "}" or next_char == "]" then
        i = i + 1 -- drop it
      else
        out[#out + 1] = c
        i = i + 1
      end
    else
      out[#out + 1] = c
      i = i + 1
    end
  end
  return table.concat(out)
end

--- Turn JSONC into JSON.
---
--- `cru.json.decode` is strict serde_json, and devcontainer.json is JSONC in
--- practice — VS Code's own templates ship with comments and trailing commas.
--- Treating a decode failure as "this project has no devcontainer" would fall
--- through to a profile and build a different environment without saying so.
function M.strip_jsonc(text)
  return strip_trailing_commas(strip_comments(text))
end

-- ─────────────────────────────────────────────────────────────────────────────
-- Variable substitution
-- ─────────────────────────────────────────────────────────────────────────────

--- Expand the `${…}` variables this plugin can compute, refusing the rest.
---
--- Most real devcontainer.json files use at least
--- `${localWorkspaceFolderBasename}`. Left alone, the literal `${…}` reaches
--- the runtime as a directory name or an env value — so an unknown variable is
--- refused rather than passed through.
local function substitute(value, vars)
  if type(value) == "table" then
    local out = {}
    for k, v in pairs(value) do out[k] = substitute(v, vars) end
    return out
  end
  if type(value) ~= "string" then return value end
  return (value:gsub("%${([^}]*)}", function(name)
    local resolved = vars[name]
    if resolved == nil then
      error("oci: devcontainer.json uses ${" .. name .. "}, which this plugin cannot "
        .. "compute; set the value literally, or configure a [plugins.oci] profile "
        .. "for this session instead")
    end
    return resolved
  end))
end

-- ─────────────────────────────────────────────────────────────────────────────
-- Mounts
-- ─────────────────────────────────────────────────────────────────────────────

--- Convert one devcontainer mount into a `-v` spec.
---
--- devcontainer mounts are `"source=…,target=…,type=bind"` strings or the
--- equivalent object; `[plugins.oci] mounts` are already `-v` specs. The two
--- shapes must not be conflated — appending a devcontainer mount verbatim after
--- `-v` is rejected by every runtime.
local function mount_spec(entry, index)
  local fields = {}
  local function put(key, value)
    local canonical = MOUNT_KEYS[key]
    if not canonical then
      error("oci: devcontainer.json mounts[" .. index .. "] uses '" .. key
        .. "', which this plugin cannot honour")
    end
    fields[canonical] = tostring(value)
  end

  if type(entry) == "string" then
    for pair in entry:gmatch("[^,]+") do
      local key, value = pair:match("^%s*(.-)%s*=%s*(.*)$")
      if not key then
        error("oci: devcontainer.json mounts[" .. index
          .. "] is not a key=value list: " .. entry)
      end
      put(key, value)
    end
  elseif type(entry) == "table" then
    for key, value in pairs(entry) do put(key, value) end
  else
    error("oci: devcontainer.json mounts[" .. index .. "] is a " .. type(entry)
      .. ", expected a string or an object")
  end

  if not fields.source or not fields.target then
    error("oci: devcontainer.json mounts[" .. index .. "] needs both source and target")
  end
  local spec = fields.source .. ":" .. fields.target
  if fields.readonly == "true" then spec = spec .. ":ro" end
  return spec
end

-- ─────────────────────────────────────────────────────────────────────────────
-- Parsing
-- ─────────────────────────────────────────────────────────────────────────────

--- Keys of `t`, sorted.
---
--- `pairs` order is unspecified, and a refusal that names an arbitrary one of
--- several offending keys is a refusal whose message changes run to run.
local function sorted_keys(t)
  local keys = {}
  for k in pairs(t) do keys[#keys + 1] = k end
  table.sort(keys)
  return keys
end

local function quoted_list(items)
  local quoted = {}
  for i, item in ipairs(items) do quoted[i] = "'" .. item .. "'" end
  return table.concat(quoted, ", ")
end

--- Parse devcontainer.json text into the plugin's environment shape.
---
--- `dir` is the directory holding the file — `build.dockerfile` and
--- `build.context` are relative to it. Raises naming the key when the file
--- asks for something that cannot be honoured natively; sets `cli_keys` when
--- it asks for something only `@devcontainers/cli` can build.
function M.parse(text, workspace, dir)
  local decoded = cru.json.decode(M.strip_jsonc(text))
  if type(decoded) ~= "table" or #decoded > 0 then
    error("oci: devcontainer.json did not parse as a JSON object")
  end

  -- `workspaceFolder` is substituted first and separately: it is what
  -- `${containerWorkspaceFolder}` resolves to everywhere else, and it can
  -- itself contain `${localWorkspaceFolderBasename}`.
  local vars = {
    localWorkspaceFolder = workspace,
    localWorkspaceFolderBasename = basename(workspace),
  }
  local target = decoded.workspaceFolder
    and substitute(decoded.workspaceFolder, vars)
    -- The devcontainer spec's default, deliberately not the plugin's
    -- /workspace: a devcontainer image expects its own layout, and existing
    -- [plugins.oci] configs expect theirs.
    or ("/workspaces/" .. basename(workspace))
  vars.containerWorkspaceFolder = target
  vars.containerWorkspaceFolderBasename = basename(target)

  local env = { mounts = {}, env = {}, run_args = {}, workspace_folder = target }
  local cli_keys = {}

  for _, key in ipairs(sorted_keys(decoded)) do
    local value = decoded[key]
    if M.CLI_KEYS[key] then
      cli_keys[#cli_keys + 1] = key
    elseif M.IGNORED[key] or key == "workspaceFolder" then
      -- Editor-facing, or already resolved above.
    elseif key == "image" then
      env.image = substitute(value, vars)
    elseif key == "build" then
      for _, sub in ipairs(sorted_keys(value)) do
        if not BUILD_KEYS[sub] then
          error("oci: devcontainer.json sets build." .. sub
            .. ", which this plugin cannot honour")
        end
      end
      if not value.dockerfile then
        error("oci: devcontainer.json has a build block that names no dockerfile")
      end
      env.dockerfile = resolve_rel(dir, substitute(value.dockerfile, vars))
      env.build_context = resolve_rel(dir, substitute(value.context or ".", vars))
      env.build_args = value.args and substitute(value.args, vars) or nil
    elseif key == "mounts" then
      for i, entry in ipairs(value) do
        env.mounts[i] = mount_spec(substitute(entry, vars), i)
      end
    elseif key == "containerEnv" then
      env.env = substitute(value, vars)
    elseif key == "remoteUser" then
      env.user = substitute(value, vars)
    elseif key == "runArgs" then
      env.run_args = substitute(value, vars)
    else
      error("oci: devcontainer.json sets '" .. key .. "', which this plugin cannot "
        .. "honour; remove it, or configure a [plugins.oci] profile for this session "
        .. "instead of letting the container differ from the one your editor builds")
    end
  end

  if #cli_keys > 0 then
    env.cli_keys = cli_keys
  elseif not env.image and not env.dockerfile then
    error("oci: devcontainer.json names neither an image nor a build")
  end

  -- A build needs a tag to build to. Derived from the workspace so the same
  -- project reuses the same image instead of paying a rebuild per session.
  if env.dockerfile and not env.image then
    env.image = "crucible-devcontainer/" .. M.image_tag(workspace) .. ":latest"
  end
  return env
end

--- A stable, runtime-legal image tag for a workspace path.
---
--- The basename alone collides across checkouts of the same repo; the full path
--- is not a legal tag. djb2 over the path disambiguates in a fixed width.
function M.image_tag(workspace)
  local hash = 5381
  for i = 1, #workspace do
    hash = (hash * 33 + workspace:byte(i)) % 4294967296
  end
  local slug = basename(workspace):lower():gsub("[^%w._-]", "-")
  return string.format("%s-%08x", slug, hash)
end

-- ─────────────────────────────────────────────────────────────────────────────
-- Resolution
-- ─────────────────────────────────────────────────────────────────────────────

--- Whether `workspace` sits inside a git working tree.
function M.is_git_repo(workspace)
  local r = cru.shell.exec("git", { "-C", workspace, "rev-parse", "--git-dir" })
  return not not (r and r.success)
end

--- The committed text of `relative` at HEAD, or nil when HEAD has no such path.
function M.committed_text(workspace, relative)
  local r = cru.shell.exec("git", { "-C", workspace, "show", "HEAD:" .. relative })
  if r and r.success then return r.stdout end
  return nil
end

--- Any devcontainer candidate present in the working tree, or nil.
---
--- Used only to decide between "this project has none" and "this project has
--- one that is not committed" — never as a source of configuration.
function M.find_untracked(workspace)
  for _, relative in ipairs(M.CANDIDATES) do
    local path = workspace .. "/" .. relative
    if cru.fs.exists(path) then return path, relative end
  end
  return nil
end

--- The project's devcontainer environment, or nil when it has none.
---
--- Reads the **committed** file, never the working tree. A devcontainer is
--- container configuration, and the sandboxed agent can write it: `/workspace`
--- is the host workspace through a rw bind mount, so a `write_file` to
--- `.devcontainer/devcontainer.json` lands on the host and the *next* session
--- in that workspace would resolve from it. `runArgs` becomes raw runtime argv,
--- so that session starts with whatever the previous one asked for —
--- `--privileged -v /:/host` included. An agent's write is unstaged and so
--- changes nothing until a human commits it, which is the boundary this repo
--- already applies to code.
---
--- Raises when the file names something this plugin cannot honour and
--- `@devcontainers/cli` is not installed to honour it instead.
function M.resolve(workspace)
  if not workspace or workspace == "" then return nil end

  local untracked_path = M.find_untracked(workspace)

  -- No commit boundary means nothing to trust. Refusing is loud where falling
  -- through to a profile would silently build an environment the project did
  -- not ask for — the same downgrade the plugin refuses everywhere else.
  if not M.is_git_repo(workspace) then
    if untracked_path then
      error("oci: " .. untracked_path .. " exists, but " .. workspace .. " is not a git "
        .. "repository, so there is no commit boundary that makes it trustworthy. "
        .. "Container configuration is honoured only when committed, because a "
        .. "sandboxed session can write this file through its own workspace mount. "
        .. "Track the project with git and commit it, or pass an explicit isolation "
        .. "profile for this session.")
    end
    return nil
  end

  local path, dir, text
  for _, relative in ipairs(M.CANDIDATES) do
    local committed = M.committed_text(workspace, relative)
    if committed then
      path, text = workspace .. "/" .. relative, committed
      dir = dirname(path)
      break
    end
  end

  if not path then
    if untracked_path then
      error("oci: " .. untracked_path .. " is not committed, so it is not honoured. "
        .. "Container configuration is read from HEAD, because a sandboxed session "
        .. "can write this file through its own workspace mount and would otherwise "
        .. "configure the container of the next session. Commit it to apply it, or "
        .. "pass an explicit isolation profile for this session.")
    end
    return nil
  end

  local env = M.parse(text, workspace, dir)

  -- HEAD wins, but a human who just edited the file should not be left
  -- wondering why nothing changed. The status slot reports this.
  local on_disk = cru.fs.exists(path) and cru.fs.read(path) or nil
  if on_disk ~= text then env.uncommitted_drift = true end
  if env.cli_keys then
    if not cru.shell.which(M.CLI) then
      error("oci: " .. path .. " sets " .. quoted_list(env.cli_keys)
        .. ", which this plugin cannot build natively. Install @devcontainers/cli "
        .. "(`npm install -g @devcontainers/cli`) so the environment is built the way "
        .. "your editor builds it, or pass an explicit isolation profile for this "
        .. "session. Refusing rather than starting a container that differs from it.")
    end
    env.cli = true
    -- The CLI owns the image; this is the label the sharing check and the
    -- status slot show, and it is stable per workspace.
    env.image = "devcontainer(" .. path .. ")"
  end
  return env
end

--- The outcome object `devcontainer up` prints on stdout.
---
--- It is one JSON line after however much build log precedes it, so scan for
--- the last line that decodes with an `outcome`.
function M.up_result(stdout)
  local found
  for line in (stdout or ""):gmatch("[^\n]+") do
    local ok, decoded = pcall(cru.json.decode, line)
    if ok and type(decoded) == "table" and decoded.outcome then found = decoded end
  end
  return found
end

return M
