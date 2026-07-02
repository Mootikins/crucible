import { test, expect } from '@playwright/test';
import { createStory } from './_helpers/story';
import { HARNESS_KILN, setupEditorHarness, typeInEditor } from './_helpers/editor-harness';

/**
 * Story: WS-201 / WS-202 / WS-204 — open a note, edit it, save it.
 *
 * Drives the REAL editor components (EditorProvider + EditorPanel + CodeMirror)
 * via the dev-only harness at /editor-harness.html — no registry bypass (cf.
 * e2e/file-tab.spec.ts). Asserts the genuine round-trip: open → dirty ● on the
 * tab → PUT /api/notes body → clean.
 *
 * KNOWN PRODUCT GAPS (documented, not worked around — see docs/Meta/Web User
 * Stories.md WS-202/204 GAP markers):
 *  1. src/App.tsx never mounts <EditorProvider>, so the editor is unreachable in
 *     the shipped app; the harness supplies the provider.
 *  2. No UI element calls EditorContext.saveFile (no Save button / Cmd-S); the
 *     harness supplies a Save button wired to the real saveFile.
 *  3. getLanguageExtension() in EditorPanel/FileViewerPanel only handles `.md`;
 *     rust/js highlighting (WS-204) is NOT implemented despite the deps being
 *     installed. The syntax baseline below is markdown-only for that reason.
 */

const NOTE_A = {
  name: 'Note A',
  path: `${HARNESS_KILN}/Note A.md`,
  content: '# Title\n\nHello world\n\n- [[Wikilink]]\n- unicode: café ☕\n',
};

test.describe('WS-202 editor round-trip', () => {
  test('open → type → dirty ● → save → clean, with exact PUT body', async ({ page }, testInfo) => {
    const story = createStory(testInfo);
    const harness = await setupEditorHarness(page, [NOTE_A]);

    await story.step(page, 'empty editor');

    await harness.open(NOTE_A);
    await expect(page.locator('.cm-editor')).toBeVisible({ timeout: 5000 });
    await expect(page.locator('.cm-content')).toContainText('Hello world');
    // Tab present, not yet dirty.
    await expect(page.getByText('Note A.md')).toBeVisible();
    await expect(page.getByText('●')).toHaveCount(0);
    await story.step(page, 'note opened clean');

    // Edit: append text; CodeMirror fires updateListener → updateFileContent →
    // dirty=true → EditorPanel Tab renders the ● marker.
    await typeInEditor(page, ' MORE');
    await expect(page.getByText('●')).toHaveCount(1);
    await story.step(page, 'edited - dirty dot');

    // Save via the harness Save button (wired to the real EditorContext.saveFile
    // → api.saveFileContent → PUT /api/kiln/file). Assert the exact request body.
    const putPromise = page.waitForRequest(
      (r) => r.method() === 'PUT' && r.url().includes('/api/kiln/file'),
    );
    await page.getByTestId('harness-save').click();
    const put = await putPromise;
    const body = put.postDataJSON() as { path: string; content: string };
    expect(body.path).toBe(NOTE_A.path);
    // Original content round-trips untouched: heading, wikilinks, unicode.
    expect(body.content.startsWith(NOTE_A.content)).toBe(true);
    expect(body.content).toContain('[[Wikilink]]');
    expect(body.content).toContain('café ☕');
    expect(body.content).toContain('Hello world');
    // Plus the appended edit.
    expect(body.content).toContain('MORE');

    // On success dirty clears.
    await expect(page.getByText('●')).toHaveCount(0);
    expect(harness.saves[harness.saves.length - 1]?.content).toContain('MORE');
    await story.step(page, 'saved - clean');
  });

  test('save failure keeps the file dirty and surfaces an error', async ({ page }) => {
    // Re-route PUT to 500 to exercise the failure branch.
    const harness = await setupEditorHarness(page, [NOTE_A]);
    // Re-route the PUT save to 500 to exercise the failure branch (GET load is
    // still served by the harness route registered earlier).
    await page.route('**/api/kiln/file**', (route) => {
      if (route.request().method() === 'PUT') {
        return route.fulfill({ status: 500, body: 'disk full' });
      }
      return route.fallback();
    });

    await harness.open(NOTE_A);
    await expect(page.locator('.cm-editor')).toBeVisible();
    await typeInEditor(page, ' X');
    await expect(page.getByText('●')).toHaveCount(1);

    await page.getByTestId('harness-save').click();
    // Failure keeps dirty and shows the harness error (EditorContext.error()).
    await expect(page.getByTestId('harness-error')).toBeVisible({ timeout: 5000 });
    await expect(page.getByText('●')).toHaveCount(1);
  });
});

test.describe('WS-204 syntax-aware editing (markdown)', () => {
  test('markdown highlight baseline', async ({ page }, testInfo) => {
    const story = createStory(testInfo);
    const harness = await setupEditorHarness(page, [NOTE_A]);
    await harness.open(NOTE_A);
    await expect(page.locator('.cm-editor')).toBeVisible();
    // CodeMirror markdown language tokenizes the heading/list — assert the
    // language layer produced styled tokens (cm-line spans exist).
    await expect(page.locator('.cm-line').first()).toBeVisible();
    await story.step(page, 'markdown highlighted');
    await expect(page.locator('.cm-editor')).toHaveScreenshot('editor-markdown.png', {
      maxDiffPixelRatio: 0.02,
    });
  });
});
