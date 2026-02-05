# crucible-web

Browser-based chat UI for Crucible with SolidJS frontend and Axum backend.

## Stack

**Backend (Rust):**
- **Framework**: Axum HTTP server
- **Communication**: REST + SSE endpoints, bridges to daemon via JSON-RPC over Unix socket
- **Asset Serving**: Embeds frontend dist/ in release builds, serves from filesystem in debug

**Frontend (SolidJS):**
- **Framework**: SolidJS (not React/Svelte)
- **UI Components**: Solid UI (shadcn-style, Kobalte-based)
- **Styling**: Tailwind CSS
- **Build**: Vite
- **Layout**: Dockview (multi-panel)
- **Package Manager**: **bun** (not npm/yarn)

## Quick Start

From the repo root, use `just`:

```bash
# Build frontend and run server (production-like)
just web

# Or for hot-reload development:
just web-vite      # Vite dev server (localhost:5173)
just web-vite-host # Vite exposed to network
```

## Development

The `web/` directory contains the SolidJS frontend.

**Use Bun, not npm:**

```bash
cd web
bun install
bun run dev    # Vite hot-reload server (localhost:5173, proxies to :3000)
bun run build  # Build static assets to dist/
```

The Rust server (`cru serve`) embeds or serves the built assets from `web/dist/`.

## Structure

- `web/` - SolidJS frontend
  - `src/components/` - SolidJS components
  - `src/contexts/` - State management (SolidJS contexts)
  - `src/hooks/` - Reusable reactive hooks
  - `src/lib/` - Utilities and API client
  - `dist/` - Built static assets (gitignored)
- `src/` - Rust backend (Axum server)
  - `server.rs` - Axum server config
  - `assets.rs` - Static asset serving

## Key Points

- Dev server proxies `/api/*` to Axum backend (localhost:3000)
- Production: Axum serves static files from `dist/` via rust-embed
- Frontend can run standalone (mock API) for UI development
- Use SolidJS patterns (createSignal, createEffect) — not React patterns
