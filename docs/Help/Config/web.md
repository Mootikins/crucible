---
title: "Web UI Configuration"
description: Every field on the [web] config section for the browser UI served by cru web
tags:
  - help
  - config
  - web
---

# Web UI Configuration

`[web]` configures the browser UI that `cru web` serves. Every field has a default, so the
section is optional — `cru web` works with no configuration at all, serving the embedded
frontend on `http://localhost:3000`.

Add it to `~/.config/crucible/config.toml`.

## `[web]`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `port` | integer | `3000` | Port to listen on |
| `host` | string | `"127.0.0.1"` | Bind address. `0.0.0.0` exposes the UI to your network |
| `static_dir` | string | *(unset)* | Serve assets from this directory instead of the ones embedded in the binary |
| `api_key` | string | *(unset)* | Bearer token for non-localhost clients. Unset generates and persists one; `""` disables auth entirely |
| `remote_shell` | bool | `false` | Let authenticated non-localhost clients use the terminal routes |
| `enabled` | bool | `false` | **Currently unread.** `cru web` starts the server unconditionally; nothing consults this field |

```toml
[web]
port = 3000
host = "127.0.0.1"
```

`cru web` overrides `port`, `host`, `static_dir`, and `remote_shell` from the command line:

```bash
cru web --port 8080
cru web --host 0.0.0.0
cru web --static-dir ./web/dist
cru web --remote-shell
```

`--remote-shell` is additive with the config value — passing the flag turns it on, but it
cannot turn a configured `remote_shell = true` off.

## Authentication

Localhost requests never need a key. Non-localhost requests must present one as a bearer
token, or sign in once through the UI to get an HttpOnly session cookie.

The key resolves in this order:

1. `api_key` in `[web]`, if set to a non-empty string — used as-is.
2. `api_key = ""` — **auth is disabled**; every client is trusted.
3. Otherwise `~/.config/crucible/api_key`, read if it exists and is non-empty.
4. Otherwise a random key is generated and written there (mode `0600` on Unix) on first
   start.

```bash
cru web key              # print the current key
cru web key --rotate     # generate a new one (fails if api_key is set in config)
```

`--rotate` only manages the generated key file. When `api_key` is set explicitly in the
config, change it there instead.

The key is deliberately never embedded in the URLs `cru web` prints — query-string tokens
leak through browser history, server logs, and referrer headers.

## Remote shell access

The terminal routes hand out a PTY, which is unrestricted shell access on the host. They are
therefore **loopback-only by default**. Setting `remote_shell = true` lifts that restriction
for authenticated clients only, and it is fail-closed: with no API key configured (including
`api_key = ""`), `remote_shell` is ignored and the loopback restriction stays, with a warning
logged at startup.

```toml
[web]
host = "0.0.0.0"
remote_shell = true       # only takes effect because a key is in use
```

## CORS

The server allows its own origins automatically: `http://<host>:<port>`,
`http://127.0.0.1:<port>`, and `http://localhost:<port>` (plus the Vite dev server on
`http://localhost:5173` in debug builds). Add more with a comma-separated
`CRUCIBLE_CORS_ORIGINS`:

```bash
CRUCIBLE_CORS_ORIGINS="https://notes.example.com,http://192.168.1.10:3000" cru web
```

## `[web]` vs `[server]`

These are different sections. `[web]` is the browser UI above. `[server]` holds daemon-side
settings (`auto_archive_hours`) plus several TLS and request-limit fields reserved for
future use and not yet wired to any behaviour. Configuring the web UI under `[server]` has
no effect.

## See Also

- [[Help/Config/acp]] — ACP agent configuration
- [[Help/Config/permissions]] — tool permission rules, which apply to web sessions too
- [[Help/CLI/Index]] — full CLI reference
