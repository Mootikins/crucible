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
bun run dev          # Dev server with hot reload (localhost:5273)
bun run build        # Production build to dist/
bun run preview      # Preview production build
```

## Architecture

```
src/
├── windowing/           # Window manager core — no import from the app (see Testing: boundary.test.ts)
│   ├── model/             # WindowState and the node types, the tree helpers, the v10 layout serializer
│   ├── store/             # The store, the WindowPolicy seam, and the tab/layout/floating actions
│   ├── components/        # WindowManager, EdgeHost, Ribbon, DockedBody, Pane, TabBar, FloatingWindow, etc.
│   ├── reveal/             # RevealController and flyoutRect
│   └── testing/            # neutralPolicy — shared by the core's unit tests and the harness page
├── components/          # UI components
├── contexts/            # SolidJS context providers (client-local session and editor state)
├── hooks/               # Reusable reactive hooks
├── stores/              # Global state; stores/windowStore.ts configures the windowing core with the app's WindowPolicy
├── types/               # Shared types (e.g. windowTypes, now an alias layer over windowing/model/types)
└── lib/                 # Utilities, API client, non-reactive code
    └── query/       # The owner of every server entity: one hook per entity, the key factory in `keys.ts`, the four SSE roots in `routes/`, the query client in `client.ts`
```

The main UI is a **window manager**: collapsible edge panels (left/right), a main area with recursive split panes and tab groups, and floating windows. The window manager itself is domainless and lives in `src/windowing/`; the app configures it once, with one `WindowPolicy`, from `stores/windowStore.ts`. Read `docs/Meta/Architecture/Web Windowing.md` for the folder, the policy and the edge modes. Drag-and-drop uses `@thisbeyond/solid-dnd`.

**The data layer.** Every server entity has one owner in `lib/query/`, and the rules below hold across the whole app. A component never imports `lib/api` for a fetch; it imports a hook from `lib/query/`. A mutation names the keys it invalidates. Server events reach the cache through one route per stream, under `lib/query/routes/`. The typed bus in `lib/bus.ts` carries the cross-component data events. The API contract is generated: `just web-contract` writes `crates/crucible-web/openapi.json` and `src/lib/api-schema.d.ts`, and `just lint types` fails when either one is stale.

**MVVM Pattern:**
- **Model**: The windowing store (`windowing/store`) and contexts (ChatContext, WhisperContext)
- **ViewModel**: Hooks and store actions
- **View**: Components — render, emit events

## Key Dependencies

- `solid-js` — Reactive UI framework
- `@thisbeyond/solid-dnd` — Drag and drop for tabs/panes
- `@huggingface/transformers` — optional browser-side Whisper (WebGPU/WASM); the app mounts WhisperProvider inside SettingsProvider

## Development Notes

- Dev server proxies `/api/*` to Axum backend (localhost:3000)
- Production: Axum serves static files from `dist/` via rust-embed
- Frontend can run standalone (mock API) for UI development

## Testing

Three layers, all bun-driven:

| Layer | Command | CI |
|-------|---------|----|
| **Vitest** (unit, jsdom) | `bun run test` / `just web-test unit` | `test-web` job + `just ci` |
| **Playwright `ui`** (mocked API, ~78 specs) | `just web-test ui` / `bunx playwright test --project=ui` | `test-web` job + `just ci` |
| **Playwright `stories`** (user-story suites, video+trace+step screenshots) | `just web-test stories` | `test-web` job (runs with the default `bunx playwright test`) |
| **Playwright live + served** (end to end: the `cru` and the `dist` this tree just built, a real daemon, a temp kiln, a fake model server) | `just web-test live` | `test-web-live` job + `just ci` (the recipe builds `cru` and `dist` itself) |

- **Vitest gates CI** (added 2026-07). Coverage thresholds live in `vite.config.ts`.
- **Story specs** live in `e2e/stories/**`; the `stories` project sets `video/trace/screenshot: on`. `createStory(testInfo).step(page, name)` writes an ordered image sequence per story. Committed visual baselines are under `e2e/__screenshots__/` (re-included past the root `*.png` ignore). Per repo policy, EYE-VERIFY a regenerated baseline before committing — never blindly `--update-snapshots`.
- **Editor stories** drive the REAL editor via the dev-only harness at `/editor-harness.html` (`src/test-harness/editor-harness.tsx`) — not the registry-bypass in `e2e/file-tab.spec.ts`. The harness is dev-served only and never ships in `dist`.
- **Windowing core specs** live in `e2e/windowing/**` (`split`, `tabs`, `rails`, `floating`, `restore`, `modes`) and drive the dev-only harness at `/windowing-harness.html` (`src/test-harness/windowing-harness.tsx`), which mounts the window manager with a neutral policy: no app, no panel registry, no rails rule. They run in the `ui` project alongside the app specs. `e2e/fixed-rails.spec.ts` keeps the app-only rule (the last Sessions and Files tabs do not close) against the real app. `src/windowing/__tests__/boundary.test.ts` is the gate for the whole core: it fails the build the moment any file under `src/windowing/` imports from the app. See `docs/Meta/Architecture/Web Windowing.md`.
- **The `ui` tier is NOT end to end.** It mocks every API route in the browser (`e2e/helpers/mock-api.ts`, `page.route`) and the terminal socket (`page.routeWebSocket`, so a daemon on the API port cannot leak a 403 into a spec), so it proves what a component does with an answer the spec itself wrote: layout, drag, focus, keyboard. It cannot see a daemon that refuses a call. A scenario whose "then" is "the daemon accepted it" belongs in the live tier.
- **Live tier** (`playwright.live.config.ts`): `e2e/live/global-setup.ts` boots `cru web` on an isolated `$CRUCIBLE_SOCKET` against a TempDir kiln (seeded and indexed via `cru process` — `/api/kiln/notes` serves the note index, and opening a kiln deliberately does not scan it). It is STRICT and HERMETIC:
  - **Strict.** It runs the `cru` under `target/debug` (or `$CRU_BIN`, or `$CARGO_TARGET_DIR`) and serves `web/dist`, and FAILS the run when either is missing or older than its sources. There is no PATH fallback and no green skip: a tier that silently tested an installed binary reported a pass for code it never ran. `just web-test live` builds both, in that order, every time.
  - **Hermetic.** Own `HOME`, `XDG_*`, `CRUCIBLE_HOME` and `CRUCIBLE_CONFIG_DIR` under one temp dir; every inherited `CRUCIBLE_*`, provider credential and proxy variable dropped; a fake Ollama server (`fake-ollama.ts`) injected through `init.lua` as the only model the daemon can reach, proved by reading the daemon's effective config.
  - **`lane-guard.live.spec.ts`** asserts all of that from inside the run: the served bundle carries this run's `dist/live-stamp.json`, `/proc` says the web process and the daemon are the built binary, the daemon's environment points inside the temp dir and holds no credential, and a real turn appears in the fake's request log.
  - Three projects: `live` (the lane guard, the session path in `session-path.live.spec.ts`, and the kiln/notes suite; the hero specs are `testIgnore`d — they belong to `playwright.hero.config.ts`), `live-compact` (the conflict leg on a phone-shaped viewport) and `served` (`tests/*.pw.ts` — the built bundle with the Rust server's real headers; the ONLY guard for CSP/`nosniff`/`Referrer-Policy`/`/api/file/raw` regressions, which the ui tier structurally cannot see).
- **Follow-up (pre-existing):** the original e2e specs have type errors under a `src`+`e2e` typecheck (unused vars; `/src/...` dynamic imports in `file-tab.spec.ts`/`session-file-integration.spec.ts`). They predate this work and aren't covered by the `src`-scoped `bun run typecheck`; clean up if/when the ui specs are folded into the typechecked project.

## Do NOT

- Use npm or yarn (bun only)
- Import React patterns (no useState, useEffect — use createSignal, createEffect)
- Add SSR complexity (static build only)
- Call `lib/api` from a component; use a `lib/query/` hook
- Hand-edit `src/lib/api-schema.d.ts` or `crates/crucible-web/openapi.json`
