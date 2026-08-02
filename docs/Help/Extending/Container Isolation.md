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
`crucible.toml`:

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
folder it reports. When it is not, **the session is refused naming the keys that
could not be honoured**:

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

## Internal agents only

Isolation is enforceable only for **internal** agents, whose tools the daemon
dispatches — interception and the default-deny gate sit *before* execution.
An external ACP agent (`cru chat -a claude` and friends) executes tools in its
own process and reports them to the daemon as notifications; nothing the
daemon does can stop a call it learns about after the fact. Rather than let a
session look sandboxed while every tool runs on the host, the daemon
**refuses** to pair an isolation claim with an external agent: creating such
a session fails, switching an isolated session to an ACP agent is rejected,
and delegating to one from a sandboxed session is refused for the same reason.

## See Also

- [[Help/Extending/Event Hooks]] — the hook API `oci` is built on
- [[Help/Extending/Creating Plugins]] — plugin structure and `setup()`
