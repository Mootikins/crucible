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

## Testing

Three layers, all bun-driven:

| Layer | Command | CI |
|-------|---------|----|
| **Vitest** (unit, jsdom) | `bun run test` / `just web-test unit` | `test-web` job + `just ci` |
| **Playwright `chromium`** (mocked E2E, ~78 specs) | `bunx playwright test --project=chromium` | `test-web` job + `just ci` |
| **Playwright `stories`** (user-story suites, video+trace+step screenshots) | `just web-test stories` | `test-web` job (runs with the default `bunx playwright test`) |
| **Playwright live + served** (real `cru web`+daemon+temp kiln) | `just web-test live` | `test-web-live` job + `just ci` (the recipe builds `cru` and `dist` itself) |

- **Vitest gates CI** (added 2026-07). Coverage thresholds live in `vite.config.ts`.
- **Story specs** live in `e2e/stories/**`; the `stories` project sets `video/trace/screenshot: on`. `createStory(testInfo).step(page, name)` writes an ordered image sequence per story. Committed visual baselines are under `e2e/__screenshots__/` (re-included past the root `*.png` ignore). Per repo policy, EYE-VERIFY a regenerated baseline before committing — never blindly `--update-snapshots`.
- **Editor stories** drive the REAL editor via the dev-only harness at `/editor-harness.html` (`src/test-harness/editor-harness.tsx`) — not the registry-bypass in `e2e/file-tab.spec.ts`. The harness is dev-served only and never ships in `dist`.
- **Live tier** (`playwright.live.config.ts`): `e2e/live/global-setup.ts` boots `cru web` on an isolated `$CRUCIBLE_SOCKET` against a TempDir kiln (seeded and indexed via `cru process` — `/api/kiln/notes` serves the note index, and opening a kiln deliberately does not scan it) and scrubs provider credentials so no real LLM call can happen. Provide the binary via `CRU_BIN=…/target/debug/cru`; otherwise it skips (setup self-cleans on any failure path). Two projects: `live` (kiln/notes endpoints; the hero specs are `testIgnore`d — they belong to `playwright.hero.config.ts`, which provides their fake-Ollama setup) and `served` (`tests/*.pw.ts` — the built bundle with the Rust server's real headers; the ONLY guard for CSP/`nosniff`/`Referrer-Policy`/`/api/file/raw` regressions, which the mock tier structurally cannot see). This tier is deterministic (kiln/filesystem only, no LLM) and is now in CI. Because it skips *green* without a binary, the `just web-test live` tier builds `cru` (`cargo build -p crucible-cli --bin cru`) and the web assets (`bun run build`; the setup starts `cru web --static-dir <web>/dist`, since the embedded bundle is baked in at compile time — before this tier builds `dist/`) rather than leaving that to the caller — a gate that can silently pass is not a gate.
- **Follow-up (pre-existing):** the original e2e specs have type errors under a `src`+`e2e` typecheck (unused vars; `/src/...` dynamic imports in `file-tab.spec.ts`/`session-file-integration.spec.ts`). They predate this work and aren't covered by the `src`-scoped `bun run typecheck`; clean up if/when e2e is folded into the typechecked project.

## Do NOT

- Use npm or yarn (bun only)
- Import React patterns (no useState, useEffect — use createSignal, createEffect)
- Add SSR complexity (static build only)
