# Web UI server (crucible-web)

Browser-based chat UI for Crucible: Axum backend (this module) + SolidJS frontend (`crates/crucible-web/web/`).

## Stack

**Backend (Rust, this module):**
- **Framework**: Axum HTTP server, started by `cru web`
- **Communication**: REST + SSE endpoints, bridges to daemon via JSON-RPC over Unix socket
- **Asset Serving**: Embeds frontend dist/ in release builds, serves from filesystem in debug

**Frontend (SolidJS):** see `crates/crucible-web/web/AGENTS.md` — uses **bun** (not npm/yarn).

## Quick Start

From the repo root, use `just`:

```bash
just web                # hot reload (Vite) + the API behind it — the dev loop
just web-static         # build the bundle, serve it from `cru` on 0.0.0.0:3000
```

`just web` runs both processes and passes the API port through to Vite's proxy,
so `just web 3001` moves both ends. Use `web-static` when the thing under test
is the binary's own responses — CSP, nosniff, Content-Disposition — which the
dev server does not send.

`just --show web` explains the flags and the LAN host policy.

## Structure

- `crates/crucible-web/web/` - SolidJS frontend (`src/components/`, `src/contexts/`, `src/hooks/`, `src/lib/`; `dist/` is gitignored build output)
- `crates/crucible-web/src/` - Rust backend (Axum server)
  - `server.rs` - Axum server config
  - `assets.rs` - Static asset serving: embedded `web/dist` (rust-embed, path relative to this crate) in every build profile, or `--static-dir`/`[web] static_dir` to serve a directory from disk instead
  - `routes/` - REST/SSE route handlers
  - `services/` - Daemon RPC client wrapper

## Key Points

- Dev server proxies `/api/*` to Axum backend (localhost:3000)
- Production: Axum serves static files from `dist/` via rust-embed
- Frontend can run standalone (mock API) for UI development
- Use SolidJS patterns (createSignal, createEffect) — not React patterns
