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
├── components/         # UI components
│   └── windowing/      # Window manager (WindowManager, SplitPane, Pane, TabBar, EdgePanel, etc.)
├── contexts/           # SolidJS context providers (state management)
├── hooks/              # Reusable reactive hooks
├── stores/             # Global state (e.g. windowStore for layout/tabs/panels)
├── types/              # Shared types (e.g. windowTypes)
└── lib/                # Utilities, API client, non-reactive code
```

The main UI is a **window manager** (demo-style): header bar, collapsible edge panels (left/right/bottom), main area with recursive split panes and tab groups, floating windows, flyout, status bar. State is in `stores/windowStore` (Solid `createStore`); drag-and-drop uses `@thisbeyond/solid-dnd`.

**MVVM Pattern:**
- **Model**: Store (windowStore) and contexts (ChatContext, WhisperContext when re-wired)
- **ViewModel**: Hooks and store actions
- **View**: Components — render, emit events

## Key Dependencies

- `solid-js` — Reactive UI framework
- `@thisbeyond/solid-dnd` — Drag and drop for tabs/panes
- `@xenova/transformers` — Browser-side Whisper (WebGPU) when chat is re-integrated

## Development Notes

- Dev server proxies `/api/*` to Axum backend (localhost:3000)
- Production: Axum serves static files from `dist/` via rust-embed
- Frontend can run standalone (mock API) for UI development

## Do NOT

- Use npm or yarn (bun only)
- Import React patterns (no useState, useEffect — use createSignal, createEffect)
- Add SSR complexity (static build only)
