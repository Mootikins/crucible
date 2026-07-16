import { test, expect } from '@playwright/test';
import { createStory } from './_helpers/story';
import { HARNESS_KILN, setupEditorHarness } from './_helpers/editor-harness';

/**
 * Story: Obsidian-style live preview is the markdown editing default.
 *
 * Validated behaviors:
 *  1. Opening a note shows styled prose — heading sized with its `#`
 *     hidden, bold bold without `**`, inline code as a mono chip without
 *     backticks, wikilinks as pills showing display text (visual baseline).
 *  2. Clicking into a construct reveals ONLY that construct's raw source;
 *     everything else stays styled (visual baseline).
 *  3. The mode toggle switches to the raw mono source flow and back.
 */

const NOTE = {
  name: 'Live Note',
  path: `${HARNESS_KILN}/Live Note.md`,
  content: [
    '# Live Heading',
    '',
    'Some **bold** prose with `inline_code` and *emphasis*.',
    '',
    'A link to [[Other Note|the other note]].',
    '',
  ].join('\n'),
};

test.describe('Editor live preview (markdown default)', () => {
  test('styled prose by default; cursor reveals one construct; source mode is a toggle', async ({ page }, testInfo) => {
    const story = createStory(testInfo);
    const harness = await setupEditorHarness(page, [NOTE]);
    await harness.open(NOTE);
    const content = page.locator('.cm-content');
    await expect(page.locator('.cm-editor')).toBeVisible({ timeout: 5000 });

    // 1. Live preview is the default: marks hidden, constructs styled.
    // Park the cursor at the end so nothing is revealed.
    await content.click();
    await page.keyboard.press('Control+End');
    await expect(content).not.toContainText('**bold**');
    await expect(content).toContainText('bold');
    await expect(content).not.toContainText('`inline_code`');
    await expect(page.locator('.cm-lp-strong')).toHaveText('bold');
    await expect(page.locator('.cm-lp-code')).toHaveText('inline_code');
    await expect(page.locator('.cm-lp-h1')).toHaveText('Live Heading');
    await expect(content).not.toContainText('# Live Heading');
    // Aliased wikilink shows only its display text.
    await expect(page.locator('.cm-wikilink')).toHaveText('the other note');
    await story.step(page, 'live preview styled');
    await expect(page.locator('.cm-editor')).toHaveScreenshot('editor-live-preview.png', {
      maxDiffPixelRatio: 0.02,
    });

    // 2. Clicking into the inline code reveals ONLY it — bold stays styled.
    await page.locator('.cm-lp-code').click();
    await expect(content).toContainText('`inline_code`');
    await expect(content).not.toContainText('**bold**');
    await story.step(page, 'cursor reveals inline code');
    await expect(page.locator('.cm-editor')).toHaveScreenshot('editor-live-reveal.png', {
      maxDiffPixelRatio: 0.02,
    });

    // 3. Source mode: everything raw, mono, no live-preview styling.
    await page.getByTestId('mode-toggle').click();
    await expect(content).toContainText('# Live Heading');
    await expect(content).toContainText('**bold**');
    await expect(content).toContainText('[[Other Note|the other note]]');
    await expect(page.locator('.cm-lp-strong')).toHaveCount(0);
    await story.step(page, 'source mode');

    // …and back to live.
    await page.getByTestId('mode-toggle').click();
    await expect(content).not.toContainText('**bold**');
    await story.step(page, 'back to live preview');
  });
});
