# Crucible Web UI E2E Tests

End-to-end tests for the Crucible web interface using Playwright.

## Running Tests

### Prerequisites

1. Install dependencies:
   ```bash
   bun install
   ```

2. Install Playwright browsers:
   ```bash
   bunx playwright install chromium
   ```

3. Start the backend server:
   ```bash
   # From repo root
   cargo run --bin cru-server
   ```

4. Build and serve the frontend:
   ```bash
   # From web/ directory
   bun run build
   bun run preview
   ```

### Test Commands

```bash
bun run test:e2e          # Run all E2E tests (headless)
bun run test:e2e:ui       # Run with Playwright UI
bun run test:e2e:headed   # Run in headed mode (see browser)
```

### Individual Test Suites

```bash
bunx playwright test e2e/smoke.spec.ts              # Smoke tests
bunx playwright test e2e/project-session.spec.ts   # Project/session management
bunx playwright test e2e/chat.spec.ts               # Chat interface
bunx playwright test e2e/notes-browser.spec.ts     # Notes browser
bunx playwright test e2e/editor.spec.ts             # Editor functionality
```

## Test Coverage

### Smoke Tests (`smoke.spec.ts`)
- Complete user flow: project → session → note → edit
- App loads without errors
- Main UI components render
- API error handling
- Responsive layout

### Project & Session Management (`project-session.spec.ts`)
- Project selection interface
- Add new project
- Session list display
- Create new session
- Session state indicators
- Switch between sessions
- Session controls (pause/resume/end)

### Chat Interface (`chat.spec.ts`)
- Chat input area
- Type in chat input
- Send button
- Message list area
- Send message
- Microphone button for voice input

### Notes Browser (`notes-browser.spec.ts`)
- Notes panel header
- Kiln section
- Loading state
- File tree with icons
- Click to open note
- Expand/collapse folders
- Empty state
- Error state

### Editor (`editor.spec.ts`)
- Empty state when no files open
- Open file from notes browser
- File tabs for open files
- Dirty indicator when modified
- Switch between tabs
- Close tabs
- CodeMirror editor display

## Test Strategy

Tests use **mocked API responses** for reliability and speed. This means:
- No backend dependency required for most tests
- Consistent, predictable behavior
- Fast execution
- Easy to test edge cases and error states

To test against a real backend, comment out the `page.route()` mocks in individual tests.

## CI Integration

Tests are configured for CI with:
- Headless mode
- 2 retries on failure
- Single worker (sequential execution)
- Screenshots on failure
- Video recording on failure
- HTML report generation

## Debugging

### View test report
```bash
bunx playwright show-report
```

### Debug specific test
```bash
bunx playwright test --debug e2e/smoke.spec.ts
```

### View traces
```bash
bunx playwright show-trace trace.zip
```

## Flexlayout-test and playwright-cli

The **flexlayout-test** page (`/flexlayout-test.html`) is used to stabilize the core layout component library (DnD, expand/collapse, resize). You can drive it with **playwright-cli** for quick manual regression checks.

### Prerequisites

- Dev server running: `bun run dev` (default port 5173; may be 5174+ if ports are in use).
- playwright-cli installed: `bunx playwright-cli install` (or global install).

### Playwright-cli workflow

1. **Open a layout** (use the port your dev server prints):
   ```bash
   playwright-cli open "http://localhost:5173/flexlayout-test.html?layout=test_with_borders"
   ```
   Other useful layouts: `basic_drag_disabled`, `splitter_handle`, `docked_panes`.

2. **Capture element refs**:
   ```bash
   playwright-cli snapshot
   ```
   Inspect `.playwright-cli/page-*.yml` for refs (e.g. `[ref=e51]` → use `e51` in commands).

3. **Actions** (refs vary; get them from the latest snapshot):
   - **Layout switch**: Use the combobox ref and `playwright-cli select <comboboxRef> "test_with_borders"`, or click the dropdown and choose an option.
   - **Expand/collapse**: Click the border dock button (e.g. left collapse `◀`): `playwright-cli click <collapseButtonRef>`.
   - **Resize**: Main-area splitters are between center tab sets; border resize is the thin edge of an expanded border. Use `mousedown` on the splitter ref, then `mousemove` and `mouseup` to drag.
   - **DnD**: Use refs from snapshot:
     - **Built-in drag** (if it works with this layout): `playwright-cli drag <tabRef> <dropRef>`
     - **Pointer-based drag** (matches app’s pointer DnD): use `playwright-cli run-code` with the snippet below (replace port if needed).

4. **Snapshot again** after each action to confirm the UI updated (e.g. panel collapsed, tab moved).

### DnD via playwright-cli (no test suite)

The layout uses **pointer** events for drag. To verify with playwright-cli:

1. **Start dev server and open the page:**
   ```bash
   bun run dev
   playwright-cli open "http://localhost:5173/flexlayout-test.html?layout=test_with_borders" --headed
   ```

2. **Take a snapshot and get refs:**
   ```bash
   playwright-cli snapshot
   ```
   Open `.playwright-cli/page-*.yml` and find refs for a center tab (e.g. "Main Content") and a drop target (e.g. another tab or border content area).

3. **Option A – use built-in drag (two refs):**
   ```bash
   playwright-cli drag <tabRef> <dropRef>
   ```
   Example: `playwright-cli drag e42 e50` (replace with refs from your snapshot).

4. **Option B – pointer sequence via run-code** (if Option A doesn’t trigger the app’s DnD):
   ```bash
   playwright-cli run-code "const from = page.locator('[data-layout-path=\"/r0/ts0/tb0\"]'); const to = page.locator('[data-layout-path=\"/r0/ts0/content\"]'); const b1 = await from.boundingBox(); const b2 = await to.boundingBox(); if (!b1 || !b2) throw new Error('bbox'); await page.mouse.move(b1.x + b1.width/2, b1.y + b1.height/2); await page.mouse.down(); await page.mouse.move(b2.x + b2.width/2, b2.y + b2.height/2, { steps: 10 }); await page.mouse.up();"
   ```
   Adjust `data-layout-path` for your layout (e.g. `/r0/ts1/content` for second tabset; `/border/border_left/t0` for left border content after expanding).

### Success criteria

- Switch layout via dropdown.
- Drag a tab from center to a border (or between center tab sets) and see the tab move.
- Click a border collapse/expand and see the border collapse/expand.
- Drag a center splitter and see pane sizes change.

## Notes

- Tests expect the app to be running on `http://localhost:3000`
- Tests use explicit assertions (`await expect(element).toBeVisible()`) to ensure proper failures
- Tests include reasonable timeouts for async operations
- API mocking ensures tests are deterministic and fast
