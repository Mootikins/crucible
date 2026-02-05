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

## Notes

- Tests expect the app to be running on `http://localhost:3000`
- Most tests use conditional checks (`if (await element.isVisible())`) to handle dynamic UI states
- Tests include reasonable timeouts for async operations
- API mocking ensures tests are deterministic and fast
