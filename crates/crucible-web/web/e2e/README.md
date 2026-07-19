# Crucible Web UI E2E Tests

End-to-end tests for the Crucible web interface using Playwright.

## Running

### Prerequisites

1. Install dependencies (from `crates/crucible-web/web/`):
   ```bash
   bun install
   bunx playwright install chromium
   ```

2. **No backend required for most tests.** Specs use mocked API/SSE responses
   via `e2e/helpers/{mock-api.ts,mock-sse.ts}`. The Playwright config
   (`playwright.config.ts`) auto-starts `bun run dev` on port 5173, so the dev
   server is the only running prerequisite — and it starts itself.

3. **For tests that hit a real backend** (rare; comment out `page.route()`
   mocks in the spec): start the daemon separately from the repo root:
   ```bash
   cargo run -p crucible-cli -- daemon serve
   ```

### Commands

```bash
bun run test:e2e          # Headless, all specs
bun run test:e2e:ui       # Playwright UI mode
bun run test:e2e:headed   # Headed browser
bunx playwright test e2e/chat-happy-path.spec.ts   # Single spec
bunx playwright show-report                          # View last HTML report
```

## Specs (20 total)

Grouped by area. See each file for details.

### Windowing & layout (6)

| Spec                              | Covers                                                              |
| --------------------------------- | ------------------------------------------------------------------- |
| `windowing-comprehensive.spec.ts` | Broad windowing flow: split, tab, edge panel, floating window       |
| `windowing-regression.spec.ts`    | Specific past-bug regressions in the window manager                 |
| `center-resize.spec.ts`           | Splitter drag in the center tiling area resizes panes               |
| `cross-zone-dnd.spec.ts`          | Drag-and-drop between zones (tab ↔ edge ↔ floating)                 |
| `panel-placeholders.spec.ts`      | Empty/placeholder content in panels                                 |
| `tab-reorder.spec.ts`             | Reordering tabs within a tab group                                  |

### Chat (4)

| Spec                          | Covers                                                                  |
| ----------------------------- | ----------------------------------------------------------------------- |
| `chat-happy-path.spec.ts`     | Send message → see assistant reply (mocked SSE stream)                  |
| `model-switching.spec.ts`     | Change model mid-session via UI control                                 |
| `tool-call-display.spec.ts`   | Tool calls render with name, args, and result in the message stream     |
| `title-generation.spec.ts`    | Session title auto-generates after first turn                           |

### Sessions (5)

| Spec                            | Covers                                                                |
| ------------------------------- | --------------------------------------------------------------------- |
| `session-management.spec.ts`    | Session list, create, switch, delete                                  |
| `session-lifecycle.spec.ts`     | Pause / resume / end transitions                                      |
| `session-filter.spec.ts`        | Search/filter the session list                                        |
| `session-button-position.spec.ts` | Button placement in session panel doesn't shift across states       |
| `new-session-chat-tab.spec.ts`  | New session opens its chat in a tab automatically                     |

### Files & integration (2)

| Spec                              | Covers                                              |
| --------------------------------- | --------------------------------------------------- |
| `file-tab.spec.ts`                | Open file → file appears as tab                     |
| `session-file-integration.spec.ts` | Session ↔ file interactions (e.g., open from chat) |

### Other (3)

| Spec                       | Covers                                              |
| -------------------------- | --------------------------------------------------- |
| `empty-state.spec.ts`      | Initial empty state of the app on fresh load        |
| `error-handling.spec.ts`   | UI behavior under API/SSE errors                    |
| `flyout-panel.spec.ts`     | Flyout panel open/close, content rendering          |

## Test strategy

- **API mocking by default.** `e2e/helpers/mock-api.ts` and `mock-sse.ts`
  provide deterministic responses. Tests are fast and don't require the daemon.
- **Real backend possible.** Comment out `page.route()` calls in a spec to
  exercise the live daemon — useful for diagnosing mock/reality drift.
- **Fixtures** in `e2e/helpers/fixtures.ts` are shared between specs; prefer
  reusing/extending them over inlining data.

## CI

Playwright config:
- Headless, chromium only (no firefox/webkit until justified by a real bug).
- 2 retries on failure in CI.
- Single worker in CI (sequential).
- Screenshots on failure; video retained on failure; trace on first retry.
- HTML report (`bunx playwright show-report` to view locally after a run).

## Debugging

```bash
bunx playwright test --debug e2e/chat-happy-path.spec.ts
bunx playwright show-trace path/to/trace.zip
```

## Manual layout debugging with playwright-cli

The **flexlayout-test** page (`/flexlayout-test.html`) is a sandbox for the
core layout library (DnD, expand/collapse, resize). Useful for one-off
debugging without writing a spec.

### Setup

```bash
bun run dev                                    # dev server (5173+)
bunx playwright-cli install                    # if not already installed
```

### Workflow

1. **Open a layout** (port may vary):
   ```bash
   playwright-cli open "http://localhost:5173/flexlayout-test.html?layout=test_with_borders"
   ```
   Other layouts: `basic_drag_disabled`, `splitter_handle`, `docked_panes`.

2. **Snapshot to get element refs**:
   ```bash
   playwright-cli snapshot
   ```
   Refs (e.g. `[ref=e51]`) appear in `.playwright-cli/page-*.yml`.

3. **Drive actions** with the captured refs:
   - Combobox: `playwright-cli select <ref> "test_with_borders"`
   - Click: `playwright-cli click <ref>`
   - Drag: `playwright-cli drag <fromRef> <toRef>`
   - For pointer-based DnD (matches app behavior), use `run-code` with a
     manual mouse sequence — see the splitter-resize snippet below.

4. **Snapshot again** to confirm the UI updated.

### Pointer-based DnD example

```bash
playwright-cli run-code "
  const from = page.locator('[data-layout-path=\"/r0/ts0/tb0\"]');
  const to   = page.locator('[data-layout-path=\"/r0/ts0/content\"]');
  const b1 = await from.boundingBox();
  const b2 = await to.boundingBox();
  if (!b1 || !b2) throw new Error('bbox');
  await page.mouse.move(b1.x + b1.width/2, b1.y + b1.height/2);
  await page.mouse.down();
  await page.mouse.move(b2.x + b2.width/2, b2.y + b2.height/2, { steps: 10 });
  await page.mouse.up();
"
```

### Center splitter resize repro

To reproduce a splitter-resize bug in the main UI:

```bash
playwright-cli open "http://localhost:5173/"
playwright-cli run-code "$(cat e2e/scripts/center-resize-repro.js)"
```

The script targets `[data-split-id="split-root"]`, measures the first pane
width, drags the splitter 80px right, then re-measures. Output:
`{"widthBefore":..., "widthAfter":...}`. If `widthAfter > widthBefore`, the
resize path works in that run; equal widths reproduce the failure.

## Notes

- Tests target `http://localhost:5173` (Vite dev server). The Playwright
  webServer config starts it automatically; do not start it manually for CI runs.
- Use explicit assertions (`await expect(element).toBeVisible()`) — implicit
  waits hide flakes.
- Mocks make tests deterministic; keep them in sync with `web/src/lib/api.ts`
  changes.
