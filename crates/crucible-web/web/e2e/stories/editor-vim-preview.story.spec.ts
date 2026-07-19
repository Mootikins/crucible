import { test, expect } from '@playwright/test';
import { createStory } from './_helpers/story';
import { HARNESS_KILN, setupEditorHarness } from './_helpers/editor-harness';

/**
 * Story: editor modal editing (vim) + markdown preview toggle.
 *
 * Validated behaviors:
 *  1. Vim keybindings are the product DEFAULT: the buffer opens in normal
 *     mode — `x` deletes the character under the cursor instead of typing,
 *     `i` enters insert mode and text lands.
 *  2. The Edit ↔ Preview toggle renders the note through the markdown
 *     pipeline: headings render as headings, wikilinks as ember anchors
 *     with data-note (visual baseline), and toggling back returns to the
 *     source editor.
 */

const NOTE = {
  name: 'Preview Note',
  path: `${HARNESS_KILN}/Preview Note.md`,
  content: '# Preview Heading\n\nA paragraph linking [[Other Note]].\n\n- item one\n- item two\n',
};

test.describe('Editor vim mode (product default)', () => {
  test('opens in normal mode: x deletes, i inserts', async ({ page }) => {
    const harness = await setupEditorHarness(page, [NOTE], { vim: true });
    await harness.open(NOTE);
    await expect(page.locator('.cm-editor')).toBeVisible({ timeout: 5000 });

    // Land the cursor on the first character.
    await page.locator('.cm-content').click();
    await page.keyboard.press('g');
    await page.keyboard.press('g');

    // Normal mode: `x` deletes under the cursor rather than typing an x.
    await page.keyboard.press('x');
    await expect(page.locator('.cm-content')).toContainText(' Preview Heading');
    await expect(page.locator('.cm-content')).not.toContainText('# Preview Heading');

    // Insert mode: text lands.
    await page.keyboard.press('i');
    await page.keyboard.type('#');
    await expect(page.locator('.cm-content')).toContainText('# Preview Heading');
  });
});

test.describe('Editor markdown preview', () => {
  test('toggle renders the note; wikilinks stay live; toggle returns to source', async ({ page }, testInfo) => {
    const story = createStory(testInfo);
    const harness = await setupEditorHarness(page, [NOTE]);
    await harness.open(NOTE);
    await expect(page.locator('.cm-editor')).toBeVisible({ timeout: 5000 });
    await story.step(page, 'source view');

    await page.getByTestId('preview-toggle').click();
    const preview = page.getByTestId('markdown-preview');
    await expect(preview).toBeVisible();
    // Rendered, not source: a real <h1>, list items, and a data-note anchor.
    await expect(preview.locator('h1')).toHaveText('Preview Heading');
    await expect(preview.locator('li')).toHaveCount(2);
    await expect(preview.locator('[data-note="Other Note"]')).toHaveText('Other Note');
    await expect(preview).not.toContainText('# Preview Heading');
    await story.step(page, 'rendered preview');

    await expect(preview).toHaveScreenshot('editor-markdown-preview.png');

    // Back to the source editor.
    await page.getByTestId('preview-toggle').click();
    await expect(page.locator('.cm-editor')).toBeVisible();
    await expect(page.locator('.cm-content')).toContainText('# Preview Heading');
    await story.step(page, 'back to source');
  });
});
