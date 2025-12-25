# Crucible Web Frontend

> AI agent instructions for the web frontend

## Stack

- **Framework**: SolidJS (not React/Svelte)
- **UI Components**: Solid UI (shadcn-style, Kobalte-based)
- **Styling**: Tailwind CSS
- **Build**: Vite
- **Package Manager**: **bun** (not npm/yarn)

## Commands

```bash
bun install          # Install dependencies
bun run dev          # Dev server with hot reload (localhost:5173)
bun run build        # Production build to dist/
bun run preview      # Preview production build
```

## Architecture

```
src/
├── components/      # UI components (SolidJS + Solid UI)
├── contexts/        # SolidJS context providers (state management)
├── hooks/           # Reusable reactive hooks
└── lib/             # Utilities, API client, non-reactive code
```

**MVVM Pattern:**
- **Model**: Contexts (ChatContext, WhisperContext) — state + API calls
- **ViewModel**: Hooks — transform state, business logic
- **View**: Components — render, emit events

## Key Dependencies

- `@xenova/transformers` — Browser-side Whisper (WebGPU)
- `solid-js` — Reactive UI framework
- `@solid-ui/*` — Accessible component primitives

## Development Notes

- Dev server proxies `/api/*` to Axum backend (localhost:3000)
- Production: Axum serves static files from `dist/` via rust-embed
- Frontend can run standalone (mock API) for UI development

## Do NOT

- Use npm or yarn (bun only)
- Import React patterns (no useState, useEffect — use createSignal, createEffect)
- Add SSR complexity (static build only)
