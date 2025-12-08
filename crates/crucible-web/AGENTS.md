# crucible-web

Browser-based chat UI for Crucible using Svelte 5 and SvelteKit.

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

The `web/` directory contains a SvelteKit frontend.

**Use Bun, not npm:**

```bash
cd web
bun install
bun run build  # Build static assets to dist/
bun run dev    # Vite hot-reload server
```

The Rust server (`cru serve`) embeds or serves the built assets from `web/dist/`.

## Structure

- `web/` - SvelteKit frontend (Svelte 5)
  - `src/lib/` - Svelte components
  - `src/routes/` - SvelteKit routes
  - `dist/` - Built static assets (gitignored)
- `src/` - Rust backend (Axum server)
  - `server.rs` - Axum server config
  - `assets.rs` - Static asset serving (embedded in release, filesystem in debug)
