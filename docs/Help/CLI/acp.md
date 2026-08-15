---
title: ACP Agent Command
description: CLI reference for running Crucible as an ACP agent over stdio.
tags: [help, cli, acp]
---

# cru acp

Run Crucible as an [[Help/Concepts/Agent Client Protocol|Agent Client Protocol]] **agent** over stdin/stdout.
An ACP host (Zed, JetBrains, Neovim, marimo — or another Crucible instance) spawns
`cru acp` and speaks line-delimited JSON-RPC on the process's stdio. Each ACP session
maps to an ordinary daemon chat session, so Precognition, kiln tools, and session
persistence all apply: the agent a host gets is exactly the internal Crucible agent,
exposed through a different front door. Sessions show up in `cru session list`.

This is the inverse of `cru chat --acp claude`, where Crucible is the *host* and an
external agent is the subprocess.

The command is headless — it never prompts on the terminal, because an editor host has
no TTY to prompt on. Permission requests go to the host instead (see below).

## Synopsis

```
cru acp [--kiln <path>]
```

| Option | Description |
|--------|-------------|
| `--kiln <path>` | Override the kiln path |

That is the entire flag surface. The `--kiln` path (or, without the flag, the
configured kiln path) is used if it contains `.crucible/`; otherwise — including when a
`--kiln` path turns out not to be a kiln — Crucible walks up from the current directory
looking for one. If nothing is found, the command exits with an error telling you to
pass `--kiln` or run from inside a kiln.

## Wire methods

`cru acp` answers the ACP agent-side methods:

| Method | Behavior |
|--------|----------|
| `initialize` | Echoes the client's protocol version; advertises text prompts, `load_session`, and `session/close` support |
| `authenticate` | No-op — no auth methods are advertised |
| `session/new` | Creates a daemon chat session (workspace = the host's `cwd`) and configures the internal agent from your config |
| `session/prompt` | Forwards the prompt via `session.send_message`, streams the turn as `session/update` notifications, returns the stop reason |
| `session/cancel` | Cancels the in-flight turn (best-effort) |
| `session/load` | Resumes an existing daemon session by ID; history is **not** replayed as `session/update` — the host keeps its own transcript |
| `session/close` | Cancels any in-flight turn and drops the session's daemon connection |

In the other direction the agent sends `session/update` notifications and round-trips
tool permission prompts to the host via `session/request_permission`.

## Host configuration

Point any ACP host at the `cru` binary with the single argument `acp` (Zed calls this an
"agent server"; other hosts use similar command/args settings). Crucible itself can host
it — add a profile to your config:

```toml
[acp.agents.crucible]
command = "cru"
args = ["acp"]
```

then `cru chat -a crucible` runs Crucible-hosting-Crucible, which exercises both sides
of the protocol.

To smoke-test the handshake without a host, pipe a framed `initialize` in:

```bash
printf '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":1}}\n' \
  | cru acp --kiln ~/my-kiln
```

## See Also

- [[Help/Concepts/Agent Client Protocol]] — the protocol Crucible speaks here
- [[Help/CLI/session]] — the daemon sessions ACP sessions map onto
- [[Help/CLI/Index]] — full CLI command reference
