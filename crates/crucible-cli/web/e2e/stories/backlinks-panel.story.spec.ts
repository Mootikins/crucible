import { test, expect, type Page } from '@playwright/test';
import { createStory } from './_helpers/story';
import { HARNESS_KILN, setupEditorHarness } from './_helpers/editor-harness';

/**
 * Story: Backlinks panel — linked + unlinked mentions for the focused note.
 *
 * Drives the REAL BacklinksPanel next to the REAL editor (EditorProvider +
 * EditorPanel + CodeMirror) via the dev harness with `?backlinks=1`.
 *
 * Validated behaviors:
 *  1. Focusing a note fetches `/api/backlinks` for its kiln-relative key.
 *  2. Linked mentions render source-note title + path (visual baseline).
 *  3. Unlinked mentions render with a one-click Link action.
 *  4. Clicking Link rewrites the OPEN EDITOR BUFFER: the plain-text mention
 *     becomes a [[wikilink]], the file goes dirty, and the suggestion leaves
 *     the list (visual baseline of the rewritten, decorated editor).
 *  5. Clicking a linked mention opens that source note in the editor.
 */

const FOCUSED = {
  name: 'notes/focused',
  path: `${HARNESS_KILN}/notes/focused.md`,
  content: 'Other Note is mentioned here without a link.\n',
};

const LINKER = {
  name: 'notes/linker',
  path: `${HARNESS_KILN}/notes/linker.md`,
  content: '# Linker Note\n\nPoints to [[focused]].\n',
};

const BACKLINKS_RESPONSE = {
  note: { path: 'notes/focused.md', abs_path: FOCUSED.path, title: 'Focused Note' },
  linked: [
    {
      name: 'linker',
      path: 'notes/linker.md',
      abs_path: LINKER.path,
      title: 'Linker Note',
    },
  ],
  unlinked: [{ mention: 'Other Note', target: 'Other Note', offset: 0 }],
};

async function setupBacklinksRoutes(page: Page) {
  await page.route('**/api/config', (r) =>
    r.fulfill({ json: { kiln_path: HARNESS_KILN } }),
  );
  await page.route('**/api/backlinks**', (route) => {
    const note = new URL(route.request().url()).searchParams.get('note') ?? '';
    if (note !== 'notes/focused.md') {
      return route.fulfill({ status: 404, body: 'not found' });
    }
    return route.fulfill({ json: BACKLINKS_RESPONSE });
  });
}

test.describe('Backlinks panel', () => {
  test('shows linked + unlinked mentions and one-click links a mention', async ({ page }, testInfo) => {
    const story = createStory(testInfo);
    await setupBacklinksRoutes(page);
    const harness = await setupEditorHarness(page, [FOCUSED, LINKER], { backlinks: true });

    await harness.open(FOCUSED);
    await expect(page.locator('.cm-editor')).toBeVisible({ timeout: 5000 });

    // 1-3. Panel resolved the focused note and rendered both sections.
    const panel = page.getByTestId('harness-backlinks');
    await expect(panel.getByTestId('backlinks-note-title')).toHaveText('Focused Note');
    await expect(panel.getByText('Linked mentions (1)')).toBeVisible();
    const linkedRow = panel.getByTestId('backlinks-linked-item');
    await expect(linkedRow).toHaveCount(1);
    await expect(linkedRow).toContainText('Linker Note');
    await expect(linkedRow).toContainText('notes/linker.md');
    await expect(panel.getByText('Unlinked mentions in this note (1)')).toBeVisible();
    await expect(panel.getByTestId('backlinks-unlinked-item')).toContainText('Other Note');
    await story.step(page, 'panel populated');

    // Visual: the whole panel — header, linked section, unlinked section.
    await expect(panel).toHaveScreenshot('backlinks-panel.png', {
      maxDiffPixelRatio: 0.02,
    });

    // 4. One-click link insertion rewrites the open buffer. The unfocused
    // editor's cursor sits at 0, touching the fresh link, so live preview
    // shows its raw source; parking the cursor elsewhere styles it into a
    // pill with the [[ ]] marks hidden.
    await panel.getByTestId('backlinks-link-button').click();
    await expect(page.locator('.cm-content')).toContainText(
      '[[Other Note]] is mentioned here without a link.',
    );
    await page.locator('.cm-content').click();
    await page.keyboard.press('Control+End');
    await expect(page.locator('.cm-content')).toContainText(
      'Other Note is mentioned here without a link.',
    );
    await expect(page.locator('.cm-content')).not.toContainText('[[Other Note]]');
    const pill = page.locator('.cm-wikilink').first();
    await expect(pill).toBeVisible();
    await expect(pill).toHaveAttribute('data-note', 'Other Note');
    // Buffer went dirty (unsaved-dot on the editor tab).
    await expect(page.getByText('●')).toHaveCount(1);
    // The applied suggestion left the list.
    await expect(panel.getByTestId('backlinks-unlinked-item')).toHaveCount(0);
    await expect(panel.getByText('Unlinked mentions in this note (0)')).toBeVisible();
    await story.step(page, 'mention converted to wikilink');

    // Visual: the rewritten buffer with the new wikilink decorated.
    await expect(page.locator('.cm-editor')).toHaveScreenshot('backlinks-after-link.png', {
      maxDiffPixelRatio: 0.02,
    });

    // 5. Linked mention opens the source note in the editor.
    await linkedRow.click();
    await expect(
      page.getByTestId('editor-tab').filter({ hasText: 'linker.md' }),
    ).toBeVisible({ timeout: 5000 });
    await expect(page.locator('.cm-content')).toContainText('Points to');
    await story.step(page, 'linked mention opened');
  });

  test('shows an empty state when no note is focused', async ({ page }) => {
    await setupBacklinksRoutes(page);
    await setupEditorHarness(page, [FOCUSED], { backlinks: true });

    const panel = page.getByTestId('harness-backlinks');
    await expect(panel.getByTestId('backlinks-empty')).toContainText('Open a note');
  });
});
