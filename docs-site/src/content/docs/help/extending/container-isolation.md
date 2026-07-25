---
title: "Container Isolation"
description: "Run agent workspace tools inside an OCI container with the oci plugin"
---

The bundled `oci` plugin runs the agent's workspace tools — `bash`, `read_file`,
`write_file`, `edit_file`, `glob`, `grep` — inside a container instead of on the
host. It is also the reference implementation for [Event Hooks](./event-hooks/):
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
with the session's workspace bind-mounted at `/workspace`, and removes it on
session end.

## Options

| Key | Default | Meaning |
|---|---|---|
| `image` | — | Image to run. **Required**; without it the plugin does nothing. |
| `runtime` | auto | `podman`, `docker` or `nerdctl`. Auto-detected in that order when unset. |
| `dockerfile` | — | Build `image` from this Dockerfile (workspace as context) before starting. |
| `mounts` | `[]` | Extra `-v` specs, e.g. `["/cache:/cache:ro"]`. |
| `env` | `{}` | Environment variables set inside the container. |
| `userns` | auto | `--userns` value. See below. `false` disables it. |
| `build_timeout` | `900` | Seconds allowed for an image build. |
| `start_timeout` | `300` | Seconds allowed for the image pull and for `run`. |

A configured `runtime` that is not installed is an error, not a cue to fall back
— substituting a different runtime would silently change the isolation you asked
for.

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

## See Also

- [Event Hooks](./event-hooks/) — the hook API `oci` is built on
- [Creating Plugins](./creating-plugins/) — plugin structure and `setup()`
