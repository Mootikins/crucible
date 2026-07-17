import { test, expect, type Page } from '@playwright/test';
import { createStory } from './_helpers/story';
import { HARNESS_KILN, setupEditorHarness } from './_helpers/editor-harness';

/**
 * Story: wikilink intelligence in the editor.
 *
 * Drives the REAL CodeMirror wikilink extension (decorations + follow
 * gestures) and the REAL app-wide hover preview inside the editor harness.
 *
 * Validated behaviors:
 *  1. `[[wikilinks]]` in a markdown buffer are decorated (.cm-wikilink with
 *     data-note) — visual baseline of the styled link.
 *  2. Hovering a decorated link floats the note-preview card with the target
 *     note's title, path, and content excerpt — visual baseline of the card.
 *  3. Ctrl/Cmd+Click follows the link: the target note opens as a tab.
 *  4. Mod-Enter with the cursor inside a link follows it too.
 */

const NOTE_A = {
  name: 'Note A',
  path: `${HARNESS_KILN}/Note A.md`,
  content: 'Start here, then read [[Other Note]] for the details.\n',
};

const OTHER = {
  name: 'Other Note',
  path: `${HARNESS_KILN}/Other Note.md`,
  content: '# Other Note\n\nThe target of the wikilink jump.\n',
};

async function setupNoteResolution(page: Page) {
  await page.route('**/api/config', (r) => r.fulfill({ json: { kiln_path: HARNESS_KILN } }));
  // getNote: /api/notes/{name}?kiln= → metadata (path is what nav/preview use).
  await page.route('**/api/notes/**', (route) => {
    const url = new URL(route.request().url());
    const name = decodeURIComponent(url.pathname.replace('/api/notes/', ''));
    if (name.toLowerCase() !== 'other note') {
      return route.fulfill({ status: 404, body: 'not found' });
    }
    return route.fulfill({
      json: {
        name: 'Other Note',
        path: OTHER.path,
        title: 'Other Note',
        tags: [],
        updated_at: '2026-01-01T00:00:00Z',
      },
    });
  });
}

test.describe('Editor wikilink navigation', () => {
  test('decorates, previews on hover, and Ctrl+Click follows', async ({ page }, testInfo) => {
    const story = createStory(testInfo);
    await setupNoteResolution(page);
    const harness = await setupEditorHarness(page, [NOTE_A, OTHER]);

    await harness.open(NOTE_A);
    await expect(page.locator('.cm-editor')).toBeVisible({ timeout: 5000 });

    // 1. The link is decorated with the resolution target. Live preview
    // (the markdown default) hides the [[ ]] marks; the buffer still holds
    // them — clicking into the link reveals the raw source.
    const link = page.locator('.cm-wikilink');
    await expect(link).toHaveCount(1);
    await expect(link).toHaveAttribute('data-note', 'Other Note');
    await expect(link).toHaveText('Other Note');
    // Clicking into the link reveals its raw source (the mark splits into
    // multiple spans around the revealed brackets — assert on the content).
    await link.click();
    await expect(page.locator('.cm-content')).toContainText('[[Other Note]]');
    // Park the cursor away from the link so the styled form is back for
    // the baseline — and the pointer too, so the hover card doesn't leak
    // into the screenshot.
    await page.locator('.cm-content').press('End');
    await expect(page.locator('.cm-wikilink').first()).toHaveText('Other Note');
    await page.mouse.move(0, 400);
    await expect(page.getByTestId('wikilink-preview')).toBeHidden();
    await expect(page.locator('[data-window-id]')).toHaveCount(0, { timeout: 5000 });
    await story.step(page, 'wikilink decorated');
    await expect(page.locator('.cm-editor')).toHaveScreenshot('editor-wikilink-decorated.png', {
      maxDiffPixelRatio: 0.02,
    });

    // 2. Hover spawns a transient floating editor window (Hover Editor).
    await link.hover();
    const popover = page.locator('[data-window-id]');
    await expect(popover).toBeVisible({ timeout: 5000 });
    await expect(popover).toContainText('Other Note');
    await expect(popover.locator('.cm-content')).toContainText(
      'The target of the wikilink jump.',
      { timeout: 5000 },
    );
    await story.step(page, 'hover popover');
    await expect(popover).toHaveScreenshot('editor-wikilink-hover-preview.png', {
      maxDiffPixelRatio: 0.02,
    });

    // Park the pointer away so the popover closes before the click-through.
    await page.mouse.move(4, 400);
    await expect(popover).toHaveCount(0, { timeout: 5000 });

    // 3. Ctrl/Cmd+Click follows the link into a new editor tab.
    await link.click({ modifiers: ['ControlOrMeta'] });
    await expect(
      page.getByTestId('editor-tab').filter({ hasText: 'Other Note.md' }),
    ).toBeVisible({ timeout: 5000 });
    await expect(page.locator('.cm-content').first()).toContainText(
      'The target of the wikilink jump.',
    );
    await story.step(page, 'ctrl+click followed');
  });

  test('Mod-Enter follows the link under the cursor', async ({ page }) => {
    await setupNoteResolution(page);
    const harness = await setupEditorHarness(page, [NOTE_A, OTHER]);

    await harness.open(NOTE_A);
    await expect(page.locator('.cm-editor')).toBeVisible({ timeout: 5000 });

    // Plain click inside the link only places the cursor (no navigation)...
    const otherTab = page.getByTestId('editor-tab').filter({ hasText: 'Other Note.md' });
    await page.locator('.cm-wikilink').click();
    await expect(otherTab).toHaveCount(0);

    // ...then Mod-Enter follows it.
    await page.keyboard.press('ControlOrMeta+Enter');
    await expect(otherTab).toBeVisible({ timeout: 5000 });
    await expect(page.locator('.cm-content')).toContainText('The target of the wikilink jump.');
    // Following a link is navigation, not an edit — the source stays clean.
    await expect(page.getByText('●')).toHaveCount(0);
  });
});
