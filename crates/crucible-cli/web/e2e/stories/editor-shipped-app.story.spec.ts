import { test, expect, type Route } from '@playwright/test';
import { createStory } from './_helpers/story';
import { setupBasicMocks } from '../helpers/mock-api';

/**
 * Story: WS-202 through the SHIPPED app (bugs 3/4/8 fixed).
 *
 * Unlike editor-roundtrip.story.spec.ts (which drives the dev-only
 * /editor-harness.html), this spec mounts the REAL App at '/' and exercises the
 * genuine product path:
 *   - App.tsx now mounts <EditorProvider> (bug 3), so FileViewerPanel resolves a
 *     real EditorContext instead of the noop fallback.
 *   - the `crucible:open-file` product event (handled in App.tsx) opens a 'file'
 *     tab via openFileInEditor — the same function FilesPanel's click calls; the
 *     real FileViewerPanel renders (registry NOT bypassed, cf. file-tab.spec.ts).
 *   - Content loads via GET /api/kiln/file (bug 8) — get_note_by_name returns no
 *     content, so the old getNote path always yielded an empty editor.
 *   - The FileViewerPanel Save button (bug 4) is wired to EditorContext.saveFile
 *     → saveFileContent → PUT /api/kiln/file (path-based, symmetric with load).
 */

const KILN = '/home/user/.crucible/kiln';
const FILE_PATH = `${KILN}/from-tui.md`;
const INITIAL = 'terminal was here\n';

test.describe('WS-202 editor round-trip (shipped App)', () => {
  test('open via product path → content loads → edit → dirty → Save → PUT', async ({ page }, testInfo) => {
    const story = createStory(testInfo);
    await setupBasicMocks(page, { sessions: [] });

    const saves: Array<{ path: string; content: string }> = [];

    // Load path (bug 8): GET /api/kiln/file returns the file bytes.
    await page.route('**/api/kiln/file**', (route: Route) => {
      if (route.request().method() === 'GET') {
        return route.fulfill({ json: { content: INITIAL } });
      }
      // Save path (bug 4): PUT /api/kiln/file records the write.
      if (route.request().method() === 'PUT') {
        const body = route.request().postDataJSON() as { path: string; content: string };
        saves.push(body);
        return route.fulfill({ status: 200, body: '' });
      }
      return route.continue();
    });

    await page.goto('/');

    // Open the file through the product's own file-open function (what
    // FilesPanel.handleFileClick calls). Registry is left intact so the REAL
    // FileViewerPanel renders under the REAL EditorProvider.
    await page.evaluate(
      ({ filePath, fileName }) => {
        window.dispatchEvent(
          new CustomEvent('crucible:open-file', { detail: { path: filePath, name: fileName } }),
        );
      },
      { filePath: FILE_PATH, fileName: 'from-tui.md' },
    );

    // Content hydrates through the real EditorContext → CodeMirror.
    await expect(page.locator('.cm-editor')).toBeVisible({ timeout: 10000 });
    await expect(page.locator('.cm-content')).toContainText('terminal was here');
    // Clean on open: Save disabled, no dirty indicator.
    await expect(page.getByTestId('file-save')).toBeDisabled();
    await expect(page.getByTestId('file-dirty-indicator')).toHaveCount(0);
    await story.step(page, 'file opened clean');

    // Edit: append text at end of document.
    await page.locator('.cm-content').first().click();
    await page.keyboard.press('ControlOrMeta+End');
    await page.keyboard.type('browser was here\n');

    // Dirty state surfaces: indicator shown, Save enabled.
    await expect(page.getByTestId('file-dirty-indicator')).toBeVisible();
    await expect(page.getByTestId('file-save')).toBeEnabled();
    await story.step(page, 'edited - dirty');

    // Save through the product Save button → real saveFile → PUT /api/kiln/file.
    const putPromise = page.waitForRequest(
      (r) => r.method() === 'PUT' && r.url().includes('/api/kiln/file'),
    );
    await page.getByTestId('file-save').click();
    const put = await putPromise;
    const body = put.postDataJSON() as { path: string; content: string };
    expect(body.path).toBe(FILE_PATH);
    expect(body.content).toContain('terminal was here');
    expect(body.content).toContain('browser was here');

    // On success the panel returns to clean.
    await expect(page.getByTestId('file-dirty-indicator')).toHaveCount(0);
    await expect(page.getByTestId('file-save')).toBeDisabled();
    expect(saves[saves.length - 1]?.content).toContain('browser was here');
    await story.step(page, 'saved - clean');
  });

  test('Cmd/Ctrl-S saves without clicking the button', async ({ page }) => {
    await setupBasicMocks(page, { sessions: [] });
    await page.route('**/api/kiln/file**', (route) => {
      const m = route.request().method();
      if (m === 'GET') return route.fulfill({ json: { content: INITIAL } });
      if (m === 'PUT') return route.fulfill({ status: 200, body: '' });
      return route.continue();
    });

    await page.goto('/');
    await page.evaluate(
      ({ filePath, fileName }) => {
        window.dispatchEvent(
          new CustomEvent('crucible:open-file', { detail: { path: filePath, name: fileName } }),
        );
      },
      { filePath: FILE_PATH, fileName: 'from-tui.md' },
    );
    await expect(page.locator('.cm-editor')).toBeVisible({ timeout: 10000 });

    await page.locator('.cm-content').first().click();
    await page.keyboard.press('ControlOrMeta+End');
    await page.keyboard.type(' EDIT');
    await expect(page.getByTestId('file-save')).toBeEnabled();

    const putPromise = page.waitForRequest(
      (r) => r.method() === 'PUT' && r.url().includes('/api/kiln/file'),
    );
    await page.keyboard.press('ControlOrMeta+s');
    await putPromise;
    await expect(page.getByTestId('file-save')).toBeDisabled();
  });
});
