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
    '---',
    'tags: [kiln, live]',
    '---',
    '# Live Heading',
    '',
    'Some **bold** prose with `inline_code` and *emphasis*.',
    '',
    'A link to [[Other Note|the other note]].',
    '',
    '| Col A | Col B |',
    '| ----- | ----- |',
    '| one   | two   |',
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
    // Frontmatter stays raw mono YAML (delimiters visible, no prose styling).
    await expect(page.locator('.cm-lp-frontmatter')).toHaveCount(3);
    await expect(content).toContainText('tags: [kiln, live]');
    // Prose wraps instead of scrolling horizontally.
    await expect(content).toHaveClass(/cm-lineWrapping/);
    // The markdown table renders as a real HTML table.
    const table = page.getByTestId('lp-table');
    await expect(table.locator('th').first()).toHaveText('Col A');
    await expect(table.locator('td').first()).toHaveText('one');
    await expect(content).not.toContainText('| ----- |');
    await story.step(page, 'live preview styled');
    await expect(page.locator('.cm-editor')).toHaveScreenshot('editor-live-preview.png');

    // 2. Clicking into the inline code reveals ONLY it — bold stays styled.
    await page.locator('.cm-lp-code').click();
    await expect(content).toContainText('`inline_code`');
    await expect(content).not.toContainText('**bold**');
    await story.step(page, 'cursor reveals inline code');
    await expect(page.locator('.cm-editor')).toHaveScreenshot('editor-live-reveal.png');

    // 3. Clicking the rendered table drops the cursor in and reveals raw
    // markdown for editing.
    await table.click();
    await expect(content).toContainText('| ----- |');
    await story.step(page, 'table revealed for editing');

    // 4. Source mode: everything raw, mono, no live-preview styling.
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

  test('callouts render as admonition blocks; cursor reveals raw source', async ({ page }, testInfo) => {
    const story = createStory(testInfo);
    const CALLOUT_NOTE = {
      name: 'Callout Note',
      path: `${HARNESS_KILN}/Callout Note.md`,
      content: [
        '# Callouts',
        '',
        'Prose before.',
        '',
        '> [!warning] Mind the gap',
        '> Callout body with **bold** prose.',
        '',
        '> [!tip]',
        '> Default title comes from the type.',
        '',
        '> [!note]- Folded away',
        '> Hidden until expanded.',
        '',
      ].join('\n'),
    };
    const harness = await setupEditorHarness(page, [CALLOUT_NOTE]);
    await harness.open(CALLOUT_NOTE);
    const content = page.locator('.cm-content');
    await expect(page.locator('.cm-editor')).toBeVisible({ timeout: 5000 });
    await content.click();
    await page.keyboard.press('Control+End');

    // 1. Fancy admonitions: icon + colored title row, raw `> [!type]` hidden.
    // (Attribute/tag/text locators only — the .callout markup carries no
    // roles/testids, and story specs may not select by raw CSS class.)
    const callouts = page.getByTestId('lp-callout');
    await expect(callouts).toHaveCount(3);
    await expect(callouts.nth(0).locator('[data-callout="warning"]')).toBeVisible();
    await expect(callouts.nth(0).getByText('Mind the gap')).toBeVisible();
    // The title-row icon renders (aria-hidden span, colored via CSS mask).
    await expect(callouts.nth(0).locator('span[aria-hidden="true"]')).toBeVisible();
    await expect(content).not.toContainText('[!warning]');
    // Untitled callout falls back to its capitalized type.
    await expect(callouts.nth(1).getByText('Tip', { exact: true })).toBeVisible();
    // `[!note]-` renders a collapsed <details>.
    const folded = callouts.nth(2).locator('details');
    await expect(folded.getByText('Folded away')).toBeVisible();
    await expect(folded).toHaveJSProperty('open', false);
    await story.step(page, 'callouts rendered');
    await expect(page.locator('.cm-editor')).toHaveScreenshot('editor-live-callouts.png');

    // 2. Clicking a foldable title toggles it — without revealing the source.
    await folded.locator('summary').click();
    await expect(folded).toHaveJSProperty('open', true);
    await expect(callouts).toHaveCount(3);
    await story.step(page, 'foldable toggled open');

    // 3. Clicking a callout body drops the cursor in and reveals raw markdown.
    await callouts.nth(0).click();
    await expect(content).toContainText('> [!warning] Mind the gap');
    await expect(callouts).toHaveCount(2);
    await story.step(page, 'callout revealed for editing');
  });
});
