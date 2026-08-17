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
cru acp [--kiln <name|path>]
```

| Option | Description |
|--------|-------------|
| `--kiln <name|path>` | The kiln to attach: the name of a `[kilns]` entry, or a directory |

That is the entire flag surface. `--kiln` takes either reading: a bare word is looked up
as a registry name, and anything that resolves to a directory is registered under a
derived name if no entry claims it already.

**An unusable `--kiln` is an error, not a fallback.** A value that is neither a known
name nor a usable directory exits with a message naming both readings. It does *not*
fall through to searching the current directory — a mistyped name silently attaching a
different kiln is exactly the confusion this refuses to create.

Without the flag, the configured kiln is used when it contains `.crucible/`. Failing
that, Crucible walks up from the current directory looking for one, and **registers what
it finds** — so running `cru acp` inside an unregistered kiln appends a `[kilns]` entry
to your config file. That is deliberate: a discovered directory with no entry would
otherwise produce a session attached to no kiln at all. If nothing is found, the command
exits telling you to pass `--kiln <name|path>` or run from inside a kiln.

Give a directory a name of your choosing with `cru kiln register` ([[Help/CLI/kiln]])
before attaching it, if you would rather not take the derived one.

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
