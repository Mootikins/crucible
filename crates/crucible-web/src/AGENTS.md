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
just web                          # build frontend, serve on 0.0.0.0:3000
cd crates/crucible-web/web && bun run dev   # hot reload, proxies /api to :3000
```

`just --show web` explains the flags, the LAN host policy, and why the dev
server's proxy pins port 3000.

## Structure

- `crates/crucible-web/web/` - SolidJS frontend (`src/components/`, `src/contexts/`, `src/hooks/`, `src/lib/`; `dist/` is gitignored build output)
- `crates/crucible-web/src/` - Rust backend (Axum server)
  - `server.rs` - Axum server config
  - `assets.rs` - Static asset serving (rust-embed folder is `web/dist` relative to the cli crate)
  - `routes/` - REST/SSE route handlers
  - `services/` - Daemon RPC client wrapper

## Key Points

- Dev server proxies `/api/*` to Axum backend (localhost:3000)
- Production: Axum serves static files from `dist/` via rust-embed
- Frontend can run standalone (mock API) for UI development
- Use SolidJS patterns (createSignal, createEffect) — not React patterns
