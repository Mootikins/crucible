import { test, expect } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';
import { appReady, openSession } from './helpers/nav';

/**
 * E2E: Inline proposed-edit diff in the real editor.
 *
 * `openFileWithDiff(path, original, proposed)` — what the ToolCard "Open in
 * editor" button calls — opens (or focuses) the file and overlays the proposed
 * content as a unified-merge inline diff (green add / red del, per-hunk
 * accept/reject in the gutter) with a "Reviewing proposed change" bar. Dismiss
 * clears it. The editor loads current content via GET /api/kiln/file.
 */

const CURRENT = 'line one\nline two\nline three\n';
const PROPOSED = 'line one\nLINE TWO CHANGED\nline three\nline four added\n';

test.describe('Inline diff in editor', () => {
  test('opens a file with an inline merge diff and dismisses it', async ({ page }) => {
    await setupBasicMocks(page);

    // The file viewer loads the current on-disk content here.
    await page.route('**/api/kiln/file**', (route) =>
      route.fulfill({ json: { content: CURRENT } }),
    );

    await page.goto('/');
    await appReady(page);

    // Trigger the exact capability the "Open in editor" button invokes.
    await page.evaluate(
      async ({ path, original, proposed }) => {
        const { openFileWithDiff } = await import('/src/lib/file-actions.ts');
        openFileWithDiff(path, original, proposed, 'app.ts');
      },
      { path: '/proj/app.ts', original: CURRENT, proposed: PROPOSED },
    );

    // The review banner appears…
    await expect(page.getByText('Reviewing proposed change')).toBeVisible({ timeout: 15000 });
    // …and the editor shows the inline unified-merge diff (changed lines).
    await expect(page.locator('.cm-changedLine').first()).toBeVisible({ timeout: 15000 });
    // The proposed content is what's shown.
    await expect(page.locator('.cm-content')).toContainText('LINE TWO CHANGED');

    // Dismiss returns the editor to the plain file (banner + merge chunks gone)
    // and restores the on-disk content — nothing was accepted.
    await page.getByRole('button', { name: 'Dismiss' }).click();
    await expect(page.getByText('Reviewing proposed change')).toHaveCount(0);
    await expect(page.locator('.cm-changedLine')).toHaveCount(0);
    await expect(page.locator('.cm-content')).not.toContainText('LINE TWO CHANGED');
    await expect(page.locator('.cm-content')).toContainText('line two');
  });

  /**
   * Accepting a hunk edits the real buffer, so an ordinary save writes the
   * accepted content to disk — that's how a reviewed change gets applied.
   */
  test('accepting a hunk and saving writes the accepted content', async ({ page }) => {
    await setupBasicMocks(page);
    await page.route('**/api/kiln/file**', (route) => {
      if (route.request().method() === 'GET') {
        route.fulfill({ json: { content: CURRENT } });
      } else {
        route.fulfill({ status: 200, body: '' });
      }
    });

    await page.goto('/');
    await appReady(page);

    await page.evaluate(
      async ({ path, original, proposed }) => {
        const { openFileWithDiff } = await import('/src/lib/file-actions.ts');
        openFileWithDiff(path, original, proposed, 'app.ts');
      },
      { path: '/proj/app.ts', original: CURRENT, proposed: PROPOSED },
    );

    await expect(page.getByText('Reviewing proposed change')).toBeVisible({ timeout: 15000 });

    // @codemirror/merge's per-chunk controls (mergeControls: true).
    const accept = page.locator('button[name="accept"]').first();
    await expect(accept).toBeVisible({ timeout: 15000 });

    const savePut = page.waitForRequest(
      (req) => req.url().includes('/api/kiln/file') && req.method() === 'PUT',
    );
    await accept.click();
    await page.locator('.cm-content').click();
    await page.keyboard.press('Control+s');

    const body = (await savePut).postDataJSON() as { path: string; content: string };
    expect(body.path).toBe('/proj/app.ts');
    // The accepted hunk is in the saved content; the file was NOT replaced
    // wholesale by the proposal — the untouched tail is still there.
    expect(body.content).toContain('LINE TWO CHANGED');
    expect(body.content).toContain('line three');
  });

  /** The other direction: a rejected hunk must not reach disk. */
  test('rejecting a hunk keeps the original text for that hunk', async ({ page }) => {
    await setupBasicMocks(page);
    await page.route('**/api/kiln/file**', (route) => {
      if (route.request().method() === 'GET') {
        route.fulfill({ json: { content: CURRENT } });
      } else {
        route.fulfill({ status: 200, body: '' });
      }
    });

    await page.goto('/');
    await appReady(page);
    await page.evaluate(
      async ({ path, original, proposed }) => {
        const { openFileWithDiff } = await import('/src/lib/file-actions.ts');
        openFileWithDiff(path, original, proposed, 'app.ts');
      },
      { path: '/proj/app.ts', original: CURRENT, proposed: PROPOSED },
    );
    await expect(page.getByText('Reviewing proposed change')).toBeVisible({ timeout: 15000 });

    const reject = page.locator('button[name="reject"]').first();
    await expect(reject).toBeVisible({ timeout: 15000 });

    const savePut = page.waitForRequest(
      (req) => req.url().includes('/api/kiln/file') && req.method() === 'PUT',
    );
    await reject.click();
    await page.locator('.cm-content').click();
    await page.keyboard.press('Control+s');

    const body = (await savePut).postDataJSON() as { content: string };
    expect(body.content).not.toContain('LINE TWO CHANGED');
    expect(body.content).toContain('line two');
  });

  /**
   * The wired path, end to end: a persisted Edit tool call in the transcript →
   * expand its card → click "Open in editor" → the real file opens with the
   * edit applied and shown as an inline merge diff. Nothing is invoked
   * directly; only the UI is driven.
   */
  test('the ToolCard "Open in editor" button opens the real file with the diff', async ({ page }) => {
    const FILE = '/home/user/project/app.ts';
    const ON_DISK = 'export function greet() {\n  return "hello";\n}\n';

    await setupBasicMocks(page, {
      sessionHistory: {
        session_id: 'test-session-001',
        total_events: 1,
        // Persisted daemon shape: {call_id, tool, args} (see ChatContext
        // history reconstruction) — replayed as a COMPLETE tool call.
        history: [
          {
            type: 'event',
            session_id: 'test-session-001',
            event: 'tool_call',
            data: {
              call_id: 'tool-edit-1',
              tool: 'Edit',
              args: {
                file_path: FILE,
                old_string: 'return "hello";',
                new_string: 'return "hello, world";',
              },
            },
            seq: 1,
          },
        ],
      },
    });

    let fileRequests = 0;
    await page.route('**/api/kiln/file**', (route) => {
      fileRequests += 1;
      route.fulfill({ json: { content: ON_DISK } });
    });

    await page.goto('/');

    // Open the session so its transcript (and the tool card) renders.
    await openSession(page, 'test-session-001');

    // Expand the collapsed tool card to reveal the diff section.
    const toolCard = page.locator('button', { hasText: 'Edit' }).first();
    await expect(toolCard).toBeVisible({ timeout: 15000 });
    await toolCard.click();

    const openBtn = page.getByTestId('tool-open-in-editor');
    await expect(openBtn).toBeVisible({ timeout: 15000 });
    await openBtn.click();

    // The file opened with the proposed content as an inline merge diff.
    await expect(page.getByText('Reviewing proposed change')).toBeVisible({ timeout: 15000 });
    await expect(page.locator('.cm-changedLine').first()).toBeVisible({ timeout: 15000 });
    await expect(page.locator('.cm-content')).toContainText('hello, world');
    // The baseline came from the file API, not from the tool args.
    expect(fileRequests).toBeGreaterThan(0);
  });

  /**
   * The proposal's baseline is the file on DISK, so staging it over a buffer
   * with unsaved edits would overwrite work the baseline never contained —
   * and Dismiss would then "restore" the disk text, losing it for good. The
   * review waits for the buffer to be clean instead.
   */
  test('will not stage a proposal over unsaved edits', async ({ page }) => {
    await setupBasicMocks(page);
    await page.route('**/api/kiln/file**', (route) => {
      if (route.request().method() === 'GET') route.fulfill({ json: { content: CURRENT } });
      else route.fulfill({ status: 200, body: '' });
    });

    await page.goto('/');
    await appReady(page);

    // Open the file and type something the user has NOT saved.
    await page.evaluate(async (path) => {
      const { openFileInEditor } = await import('/src/lib/file-actions.ts');
      openFileInEditor(path, 'app.ts');
    }, '/proj/app.ts');
    await expect(page.locator('.cm-content')).toContainText('line three', { timeout: 15000 });
    // Edit the buffer through a real CodeMirror transaction — the same path a
    // keystroke takes, without the flakiness of driving keys at a view that is
    // still mounting.
    await page.evaluate(async () => {
      const { EditorView } = await import('/node_modules/@codemirror/view/dist/index.js');
      const view = EditorView.findFromDOM(document.querySelector('.cm-editor'));
      view.dispatch({ changes: { from: 0, insert: 'ZZZ' } });
    });
    await expect(page.locator('.cm-content')).toContainText('ZZZ');

    // Now an agent proposes a change to the same file.
    await page.evaluate(
      async ({ path, original, proposed }) => {
        const { openFileWithDiff } = await import('/src/lib/file-actions.ts');
        openFileWithDiff(path, original, proposed, 'app.ts');
      },
      { path: '/proj/app.ts', original: CURRENT, proposed: PROPOSED },
    );

    // The edits survive, the proposal is held, and no merge diff is shown.
    await expect(page.getByText('Proposed change waiting')).toBeVisible({ timeout: 15000 });
    await expect(page.locator('.cm-content')).toContainText('ZZZ');
    await expect(page.locator('.cm-content')).not.toContainText('LINE TWO CHANGED');
    await expect(page.locator('.cm-changedLine')).toHaveCount(0);

    // Saving clears the way, and the review loads on its own.
    await page.locator('.cm-content').click();
    await page.keyboard.press('Control+s');
    await expect(page.getByText('Reviewing proposed change')).toBeVisible({ timeout: 15000 });
    await expect(page.locator('.cm-content')).toContainText('LINE TWO CHANGED');
  });

  /** A saved review is over: the banner goes, and the stale proposal must not
   * be re-staged the next time the file is opened. */
  test('clears the pending review once it is saved', async ({ page }) => {
    await setupBasicMocks(page);
    let onDisk = CURRENT;
    await page.route('**/api/kiln/file**', async (route) => {
      if (route.request().method() === 'GET') {
        await route.fulfill({ json: { content: onDisk } });
      } else {
        onDisk = (route.request().postDataJSON() as { content: string }).content;
        await route.fulfill({ status: 200, body: '' });
      }
    });

    await page.goto('/');
    await appReady(page);
    await page.evaluate(
      async ({ path, original, proposed }) => {
        const { openFileWithDiff } = await import('/src/lib/file-actions.ts');
        openFileWithDiff(path, original, proposed, 'app.ts');
      },
      { path: '/proj/app.ts', original: CURRENT, proposed: PROPOSED },
    );
    await expect(page.getByText('Reviewing proposed change')).toBeVisible({ timeout: 15000 });

    await page.locator('.cm-content').click();
    await page.keyboard.press('Control+s');

    // Banner and merge chunks are gone once the save lands.
    await expect(page.getByText('Reviewing proposed change')).toHaveCount(0, { timeout: 15000 });
    await expect(page.locator('.cm-changedLine')).toHaveCount(0);
    // …and reopening the file shows plain saved content, not a re-staged diff.
    await page.evaluate(async (path) => {
      const { openFileInEditor } = await import('/src/lib/file-actions.ts');
      openFileInEditor(path, 'app.ts');
    }, '/proj/app.ts');
    await expect(page.getByText('Reviewing proposed change')).toHaveCount(0);
  });
});
