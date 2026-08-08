---
title: Container Isolation
description: Run agent workspace tools inside an OCI container with the oci plugin
status: implemented
tags:
  - extending
  - plugins
  - security
  - containers
aliases:
  - OCI Plugin
---

# Container Isolation

The bundled `oci` plugin runs the agent's workspace tools — `bash`, `read_file`,
`write_file`, `edit_file`, `glob`, `grep` — inside a container instead of on the
host. It is also the reference implementation for [[Help/Extending/Event Hooks]]:
everything it does is built on `crucible.on("pre_tool_call", …)` and the
lifecycle hooks, with no container-specific Rust.

## Enabling it

Isolation is off until an image is configured. Add a `[plugins.oci]` section to
`config.toml`:

```toml
[plugins.oci]
image = "docker.io/library/alpine:latest"
```

On session start the plugin creates a container named `crucible-<session id>`
with the session's workspace bind-mounted at `/workspace` (configurable, see
below), and removes it when the last session using it ends.

## Options

| Key | Default | Meaning |
|---|---|---|
| `image` | — | Image to run. Required unless a devcontainer or a `dockerfile` supplies one. |
| `runtime` | auto | `podman`, `docker` or `nerdctl`. Auto-detected in that order when unset. |
| `dockerfile` | — | Build `image` from this Dockerfile before starting. |
| `build_context` | workspace | Build context for `dockerfile`. |
| `build_args` | `{}` | `--build-arg` values for the build. |
| `mounts` | `[]` | Extra `-v` specs, e.g. `["/cache:/cache:ro"]`. |
| `env` | `{}` | Environment variables set inside the container. |
| `run_args` | `[]` | Extra argv passed straight to `run`, e.g. `["--cap-add", "SYS_PTRACE"]`. |
| `user` | — | `--user` value. Overridden by uid mapping when that applies; see below. |
| `userns` | auto | `--userns` value. See below. `false` disables it. |
| `workspace_folder` | `/workspace` | Where the workspace is mounted inside the container. See below. |
| `devcontainer` | auto | `false` ignores this project's `devcontainer.json`; `true` opts in with no `image`. See below. |
| `devcontainer_host_access` | `false` | Honour the keys a devcontainer may not otherwise ask for (`runArgs`, `mounts`, `initializeCommand`, `features`, compose). See below. |
| `build_timeout` | `900` | Seconds allowed for an image build. |
| `start_timeout` | `300` | Seconds allowed for the image pull and for `run`. |
| `exempt` | `[]` | Host-touching tool names allowed to run on the host anyway. See below. |

A configured `runtime` that is not installed is an error, not a cue to fall back
— substituting a different runtime would silently change the isolation you asked
for.

Neither timeout buys progress, only patience: `cru.shell.exec` returns when the
process exits, so a fifteen-minute build still reports nothing until it is done.

## The mount target

`workspace_folder` is a single value that the bind mount, the container's
working directory, the paths handed to `read_file`/`write_file`/`edit_file`, and
the `glob` and `grep` search roots all read. Mounting at one path while working
in another would put every relative tool call in an empty directory, so they
cannot be set apart.

```toml
[plugins.oci]
image = "docker.io/library/rust:1-bookworm"
workspace_folder = "/workspaces/crucible"
```

It exists because devcontainers name their own — `workspaceFolder`, typically
`/workspaces/<name>` — and paths remapped against the wrong root name files the
container does not have. Unset, it stays `/workspace`.

Like `image`, it is part of the environment, so a session asking for a different
`workspace_folder` on a workspace that is already sandboxed is refused rather
than joined: the mount cannot be moved without recreating the container.

## Profiles

A bare `image` is the default profile. Named alternatives go under
`[plugins.oci.profiles]` and take the same keys. `runtime`, `exempt`,
`workspace_folder` and the two timeouts fall back to the top-level values when a
profile omits them, because they describe the box, the session's policy and the
layout you work in rather than the image itself.

```toml
[plugins.oci]
image = "docker.io/library/alpine:latest"
runtime = "podman"

[plugins.oci.profiles.rust]
image = "docker.io/library/rust:1-bookworm"

[plugins.oci.profiles.throwaway]
image = "docker.io/library/debian:trixie"
mounts = ["/var/cache/apt:/var/cache/apt:ro"]
```

Existing configs keep working: with no `profiles` table the bare `image` is
still what every session gets — unless the project has a devcontainer, which
outranks both.

## Devcontainers

If the project has a `.devcontainer/devcontainer.json` (or a top-level
`.devcontainer.json`), that file — not the profile — describes the environment.
It is read **only once something has asked for isolation**: a `[plugins.oci]`
section with an `image` or `profiles`, `devcontainer = true`, or a session
passing `isolation`. A repo that merely *contains* a devcontainer is not
containerized by that fact alone, so checking one out does not change how your
sessions run.

```toml
[plugins.oci]
devcontainer = true    # the project's environment is its devcontainer, full stop
```

Honoured natively: `image`, `build.dockerfile`, `build.context`, `build.args`,
`mounts`, `containerEnv`, `remoteUser`, `runArgs`, `workspaceFolder`, and the
`${localWorkspaceFolder}` / `${localWorkspaceFolderBasename}` /
`${containerWorkspaceFolder}` / `${containerWorkspaceFolderBasename}`
variables. Comments and trailing commas are fine — devcontainer.json is JSONC in
practice. `workspaceFolder` defaults to the devcontainer spec's
`/workspaces/<name>`, not to `/workspace`.

`features`, `dockerComposeFile` and the lifecycle commands (`initializeCommand`,
`onCreateCommand`, `updateContentCommand`, `postCreateCommand`,
`postStartCommand`, `postAttachCommand`) have no native equivalent. When
[`@devcontainers/cli`](https://github.com/devcontainers/cli) is installed the
plugin runs `devcontainer up` and adopts the container it produces, at the
folder it reports. (`dockerComposeFile` and `initializeCommand` must clear the
host gate below first — an installed CLI does not bypass it.) When it is not
installed, **the session is refused naming the keys that could not be
honoured**:

```
oci: /home/you/project/.devcontainer/devcontainer.json sets 'features',
'postCreateCommand', which this plugin cannot build natively. Install
@devcontainers/cli (`npm install -g @devcontainers/cli`) so the environment is
built the way your editor builds it, or pass an explicit isolation profile for
this session. Refusing rather than starting a container that differs from it.
```

Any other key — `remoteEnv`, `containerUser`, `workspaceMount`, anything the
spec adds next — is refused the same way. That is deliberate: the list of
honoured keys is an allowlist, so a key nobody anticipated cannot be dropped in
silence. An environment you asked for and did not get is the same failure as a
sandbox that silently did not start, one level down. The escape hatches are
`devcontainer = false`, which puts the project back on its profile, and the
per-session `isolation` param, which outranks the file.

Editor-only keys (`name`, `customizations`, `forwardPorts` and friends) are
ignored rather than refused — nothing they configure can change what a headless
agent's container is.

### What a devcontainer is not allowed to ask for

**This is a speed bump, not a security boundary.** Say that first, because the
devcontainer spec's own maintainers do: *"dev containers are not designed as a
security boundary."* They declined to gate `initializeCommand` on the grounds
that stopping one key is theatre while the rest remain, and every comparable
tool — VS Code, Zed, the reference CLI — simply trusts the file. What the list
below buys is that a devcontainer cannot *casually* hand out the host. An agent
with a shell has other routes.

These keys configure the container from outside it, and are refused by default:

| Key | Why |
|-----|-----|
| `runArgs` | Becomes raw runtime argv — `--privileged`, `-v /:/host` |
| `mounts` | Names arbitrary host paths |
| `initializeCommand` | `@devcontainers/cli` runs it **on the host**, not in the container |
| `dockerComposeFile` | The compose file it names is in the repo, and a service there can ask for `privileged: true` and `volumes: ["/:/host"]` |
| `service`, `runServices` | Only meaningful alongside a compose file, so gated with it |
| `features` | The same escape one level further out — see below |

`features` deserves its own note, because it is the one that looks innocent. A
`devcontainer-feature.json` may legally declare `privileged`, `capAdd`,
`securityOpt`, `mounts`, `init` and `entrypoint`, and the CLI merges them into
the run arguments. A feature may be referenced by a path *inside the workspace*,
so nothing vets it. Measured, not theorised: a config naming only `image` and
`"./evilfeat"` produced `--privileged`, `seccomp=unconfined`, and a bind mount
of `/` — the container could read the host's filesystem.

The reason any of this matters is that the file is inside the sandbox. A
session's workspace is bind-mounted `rw`, so the agent can edit its own
project's `devcontainer.json`, and the next session in that workspace resolves
from it. Without these keys a devcontainer still chooses its image and its
build — which decides what the sandbox *contains*, but not whether it is one.

Allow them with operator config, which lives outside the workspace where the
agent cannot write it:

```toml
[plugins.oci]
devcontainer_host_access = true
```

Note the limit: this is one flag for every project, not per project. Turning it
on for a repo that needs `runArgs` turns it on everywhere.

### The file is read from the working tree

Resolution reads `.devcontainer/devcontainer.json` as it is on disk, committed
or not — so you can change a devcontainer and test it without committing first,
which is how anyone actually edits one.

An earlier version read only the committed file, on the theory that a commit is
a human boundary. It is not one here: the workspace is bind-mounted `rw` with
`.git` inside it, so a sandboxed agent can commit as easily as it can write. The
rule cost the agent one extra command and cost everyone else the ability to
iterate. The key list above is what holds the line instead.

Reading the working tree also means this plugin and `@devcontainers/cli` read
the *same* file — the CLI's default is the same path, in the same precedence
order — so what is validated is what gets built.

`remoteUser` sets `--user`, except where uid mapping applies: `keep-id` maps the
*host* uid into the container, and running as any other id fails every workspace
write, so the numeric pin wins there. The devcontainer CLI reaches the same
place from the other side, by renumbering `remoteUser` to the host uid.

## Per-session opt-in

`session.create` takes an `isolation` param, and the daemon forwards it to the
plugin untouched as `session.isolation` on the `Session` given to
`on_session_start`. Resolution is first hit wins:

| `isolation` | Result |
|---|---|
| absent | Resolve normally — the devcontainer, then the default profile, or nothing |
| `false` | **No container**, even when the project configures one |
| `true` | The devcontainer or default profile; **refused** when neither exists |
| `"rust"` | That named profile; **refused** when no such profile exists |
| `{ image = … }` | An inline environment, same keys as a profile |

A string or an object is resolved *before* the project's devcontainer is even
read, so naming a profile is how a session opts out of a devcontainer this
plugin cannot honour.

The refusals are the same rule as everywhere else here: isolation you asked for
and did not get is never silently downgraded. `false` is the only value that
turns isolation off, and it is distinct from absent — omitting the param means
"resolve normally", not "no container".

The value is persisted with the session, so a resumed session is isolated
exactly as it was created, and a delegated child inherits its parent's — a child
that resolved isolation independently would land on the host while its parent is
sandboxed.

Because it arrives as a field on an object plugins already receive, any plugin
can read it; `oci` is simply the one that acts on it.

## From the web

The browser reaches all of this through three generic surfaces — none of which
knows what a container is:

| Surface | What it carries |
|---|---|
| `GET /api/config` → `profiles` | The named profiles this server offers, as opaque strings |
| `POST /api/session` → `isolation` | The value above, forwarded to the daemon untouched |
| `GET /api/session/{id}/status` | Every plugin's keyed status slots, verbatim |

The new-session composer shows an isolation chip — a toggle plus the profile
list — whenever `profiles` is non-empty, and omits the field entirely while the
toggle is untouched, so absent still means "resolve normally". A server with no
named profiles shows no chip: a control that could only fail is worse than none.

The status route is a plain proxy of the `session.status` RPC. Slots render as
chips keyed by `key`, labelled with `text`, toned by `level` and attributed to
`plugin`; the frontend branches on none of them. A plugin shipped tomorrow gets
a chip for free, and a failed status fetch shows as no chips rather than an
error, since it fails on every daemon reconnect.

## uid mapping

Under rootless podman the container's root maps to your host uid, so a container
running as root already writes files you own; no `--userns` flag is needed or
wanted (`keep-id` costs a one-time image re-chown, ~60s cold).

The mapping only goes wrong when the *image* runs as a non-root user: your files
appear inside as uid 0, and the container user can neither read nor write the
mount. The plugin detects that case (`image inspect` for the image's `User`) and
adds `--userns=keep-id` only then. Set `userns` explicitly to override, or
`userns = false` to suppress it.

docker's daemon runs as root and bind mounts carry real host uids, so no mapping
is applied there.

## Failure is loud

If isolation is configured but cannot be established — no runtime, image build
failed, container would not start — the plugin does **not** fall back to running
tools on the host. Every workspace tool is refused for that session with the
reason attached, so it appears in the transcript rather than only in the daemon
log. Isolation you asked for and did not get is never silently downgraded.

## What the gate covers

The default-deny gate does not refuse *every* tool — it refuses every tool that
would **touch the host**. Each tool executor declares what its tools can reach:

| Surface | Examples | Under isolation |
|---|---|---|
| Host | `bash`, `read_file`, `write_file`, `edit_file`, `glob`, `grep` | Refused unless a handler took the call, or the name is in `exempt` |
| Daemon | `semantic_search`, `read_note`, `create_note`, `list_notes`, `get_kiln_info`, the job tools | Always allowed |
| Unknown | MCP gateway tools, plugin-contributed tools | Refused, same as Host |

Knowledge tools reach daemon-side storage, not the workspace, so containerizing
a workspace says nothing about them — turning on the sandbox does not turn off
Crucible. This is a property of the classification, not a list kept in step by
hand: a kiln tool added later is `Daemon` because the executor that runs it is,
so it cannot silently break a sandboxed session.

MCP gateway tools are `Unknown` rather than `Daemon` deliberately. They run
inside the daemon process, but they are third-party code reached over a pipe,
and a filesystem MCP server is host-touching in every way that matters.

`exempt` is the escape hatch, and only for `Host` and `Unknown` — a `Daemon`
tool never needs to appear there:

```toml
[plugins.oci]
image = "docker.io/library/alpine:latest"
exempt = ["grep"]   # runs on the host, outside the container
```

## One container per workspace

Containers are keyed by **workspace**, not by session:

> One container per distinct workspace in a session tree.

A delegated subagent inherits its parent's workspace, so it registers against
the parent's existing container rather than paying a second cold start — the
bind mount would be the same directory either way. Teardown refcounts, so a
parent's container outlives its children and is removed when the last session
on that workspace ends.

Delegated children are real sessions and go through the same enforcement as any
other: plugin start hooks fire, the child gets its own isolation claim, or the
delegation is **refused**. If the parent's container is gone when a delegation
starts, the delegation fails rather than quietly running the child on the host.
Because the rule is the workspace, nothing configures any of this — and a child
given its own worktree (a distinct workspace) gets its own container for the
same reason.

The flip side: a session asking for a *different* environment on a workspace
that is already sandboxed is refused rather than joined, since joining would
hand it an image it did not ask for.

## Worktrees

A session's workspace is often a **linked git worktree** — one branch per
session is how parallel agents stay out of each other's way, and the composer
creates one when you pick a branch that has none.

A linked worktree's `.git` is not a directory but a file holding
`gitdir: /absolute/host/path/to/main/.git/worktrees/<name>`. Bind-mount the
worktree alone and that path does not exist inside the container, so *every*
git command there fails with `not a git repository`. So when the workspace's
git directory lives outside it, the main repo's `.git` is mounted too, at its
own absolute path — the pointer is baked into files that are also the human's
working tree, so rewriting them is not an option.

It is mounted read-write, because the agent commits. Its commits land on the
worktree's branch and the main checkout is untouched.

Two details worth knowing if you are debugging a mount:

- The spec uses `--mount`, not `-v`. On podman 5.8.4 a `-v` spec whose source
  and destination are **identical** — exactly this mount — is silently dropped:
  no warning, no failure, just a container whose git is broken.
- `relabel=shared` is added for podman only. Without it SELinux denies the read
  even though the mount landed. Docker and nerdctl reject the option.

- When `@devcontainers/cli` builds the environment the plugin does not own the
  mounts, so the same directory is passed to it as `devcontainer up --mount
  type=bind,source=…,target=…`. That option has no relabel key, so on SELinux
  the mount can land and the read still be denied.

Both are easy to lose silently, so the plugin runs `git rev-parse` inside the
container once at session start, on either path, and says so in the status slot
if it fails. It distinguishes the two reasons it can fail: an image that ships
no git at all (an image choice — pick a base with git, or add it in the build)
from git being present but unable to resolve the repository (the mount).

## External (ACP) agents

An **internal** agent's tools are dispatched by the daemon, so interception and
the default-deny gate sit *before* execution and a claim is enforceable by
construction.

An external ACP agent (`cru chat -a claude` and friends) executes tools in its
own process and reports them as notifications — nothing the daemon does can
stop a call it learns about after the fact. So interception cannot be the
answer, and the plugin supplies a different one: an **exec prefix**, the argv
that runs a command inside the sandbox.

```lua
crucible.require_isolation{
  session = session.id,
  plugin  = "oci",
  exec_prefix   = { "podman", "exec", "-i", "-w", "/workspace" },
  exec_env_flag = "-e",
  exec_suffix   = { "crucible-" .. session.id },
}
```

Given one, the daemon launches the agent *through* it — `podman exec -i …
npx @zed-industries/claude-agent-acp` rather than `npx …` on the host. The
agent's tools are then confined by where its process runs, and there is nothing
left to intercept. The prefix is argv, not a shell string, so the agent's own
arguments are never re-split.

The prefix comes in two halves because the agent's configured environment —
`env_overrides` on the session plus `[acp.agents.*] env` — has to go *inside*
the container. Setting it on the process the daemon spawns would set it on
podman, and the container boundary drops it there, so an agent configured with
an API key would start without one. The daemon inserts `<exec_env_flag>
NAME=VALUE` per variable between the halves, which is where a runtime wants a
command's flags: `exec crucible-x -e KEY=v agent` passes the flag to the agent
instead. Which flag that is stays here in the plugin — the daemon only fills
the hole.

Omit `exec_env_flag` and a session whose agent has configured environment is
**refused**, naming the variables that could not be delivered. Starting it
anyway would strip its credentials and fail later, somewhere unrelated.

Without one, the session is still **refused**: creating it fails, switching an
isolated session to an ACP agent is rejected, and delegating to one from a
sandboxed session is refused for the same reason. A session that merely *looks*
sandboxed is worse than one that admits it is not.

## See Also

- [[Help/Extending/Event Hooks]] — the hook API `oci` is built on
- [[Help/Extending/Creating Plugins]] — plugin structure and `setup()`
- [[Worktree Sessions]] — the workspace axis, which composes with this one
- [[Workspace and Runtime Targets]] — how a plugin contributes a run target
