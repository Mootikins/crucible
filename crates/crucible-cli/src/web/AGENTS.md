# Web UI server (cli/src/web)

Browser-based chat UI for Crucible: Axum backend (this module) + SolidJS frontend (`crates/crucible-cli/web/`).

## Stack

**Backend (Rust, this module):**
- **Framework**: Axum HTTP server, started by `cru web`
- **Communication**: REST + SSE endpoints, bridges to daemon via JSON-RPC over Unix socket
- **Asset Serving**: Embeds frontend dist/ in release builds, serves from filesystem in debug

**Frontend (SolidJS):** see `crates/crucible-cli/web/AGENTS.md` — uses **bun** (not npm/yarn).

## Quick Start

From the repo root, use `just`:

```bash
# Build frontend and run server (production-like)
just web

# Or for hot-reload development:
just web-vite      # Vite dev server (localhost:5173)
just web-vite-host # Vite exposed to network
```

## Structure

- `crates/crucible-cli/web/` - SolidJS frontend (`src/components/`, `src/contexts/`, `src/hooks/`, `src/lib/`; `dist/` is gitignored build output)
- `crates/crucible-cli/src/web/` - Rust backend (Axum server)
  - `server.rs` - Axum server config
  - `assets.rs` - Static asset serving (rust-embed folder is `web/dist` relative to the cli crate)
  - `routes/` - REST/SSE route handlers
  - `services/` - Daemon RPC client wrapper

## Key Points

- Dev server proxies `/api/*` to Axum backend (localhost:3000)
- Production: Axum serves static files from `dist/` via rust-embed
- Frontend can run standalone (mock API) for UI development
- Use SolidJS patterns (createSignal, createEffect) — not React patterns
