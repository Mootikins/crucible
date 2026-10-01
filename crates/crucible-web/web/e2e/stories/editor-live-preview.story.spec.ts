import { test, expect, type Page } from '@playwright/test';
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
 *  4. A hard-wrapped paragraph draws as one flowing paragraph that fills the
 *     prose column, and the source lines come back in source mode.
 *  5. The Properties card is ONE card: live preview and the reading view draw
 *     the same markup for a nested map, and the same raw card for frontmatter
 *     the parser cannot represent.
 *  6. Open and closed are ONE shape: the same hairline row in both states,
 *     clear of the mode toggles, with no box of the card's own.
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
    // Frontmatter renders as the Properties card (cursor starts in the
    // body); raw yaml only appears when the cursor enters the block.
    await expect(page.getByTestId('fm-card')).toBeVisible();
    await expect(page.getByTestId('fm-card')).toContainText('kiln');
    await expect(page.getByTestId('fm-card')).toContainText('live');
    await expect(content).not.toContainText('tags: [kiln, live]');
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
    await page.getByRole('button', { name: 'Source', exact: true }).click();
    await expect(content).toContainText('# Live Heading');
    await expect(content).toContainText('**bold**');
    await expect(content).toContainText('[[Other Note|the other note]]');
    await expect(page.locator('.cm-lp-strong')).toHaveCount(0);
    await story.step(page, 'source mode');

    // …and back to live.
    await page.getByRole('button', { name: 'Live preview', exact: true }).click();
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
    await content.getByText('Callouts', { exact: true }).click();
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

  test('a hard-wrapped paragraph reflows to fill the prose column', async ({ page }, testInfo) => {
    const story = createStory(testInfo);
    // Six short source lines that are ONE markdown paragraph, plus a
    // two-space hard break, which is a real line break and must survive.
    const WRAPPED_NOTE = {
      name: 'Wrapped Note',
      path: `${HARNESS_KILN}/Wrapped Note.md`,
      content: [
        'The quick brown fox jumps over',
        'the lazy dog while the sleepy',
        'cat watches from the warm sill',
        'and the kettle begins to sing',
        'somewhere behind the half shut',
        'kitchen door on a winter night.',
        '',
        'Broken here on purpose.  ',
        'Second visual line.',
        '',
      ].join('\n'),
    };
    const harness = await setupEditorHarness(page, [WRAPPED_NOTE]);
    await harness.open(WRAPPED_NOTE);
    const content = page.locator('.cm-content');
    await expect(page.locator('.cm-editor')).toBeVisible({ timeout: 5000 });
    await content.click();
    await page.keyboard.press('Control+End');

    // 1. The six source lines are one paragraph, joined by single spaces.
    await expect(content).toContainText('jumps over the lazy dog');
    await expect(content).toContainText('warm sill and the kettle');

    // 2. It fills the column: the browser wraps it into far fewer visual rows
    // than the source has lines, and each full row reaches most of the width.
    // This is the whole point — the column, not the source, sets the width.
    const flow = await page.evaluate(() => {
      const line = Array.from(document.querySelectorAll('.cm-line')).find((el) =>
        (el.textContent ?? '').startsWith('The quick brown fox'),
      );
      if (!line) return null;
      const range = document.createRange();
      range.selectNodeContents(line);
      const rects = Array.from(range.getClientRects()).filter((r) => r.width > 0);
      // One visual row per distinct top edge; its width spans its rects.
      const rows = new Map<number, { left: number; right: number }>();
      for (const r of rects) {
        const top = Math.round(r.top);
        const row = rows.get(top);
        if (row) {
          row.left = Math.min(row.left, r.left);
          row.right = Math.max(row.right, r.right);
        } else {
          rows.set(top, { left: r.left, right: r.right });
        }
      }
      const tops = Array.from(rows.keys()).sort((a, b) => a - b);
      const first = rows.get(tops[0]);
      return {
        rowCount: tops.length,
        firstRowWidth: first ? first.right - first.left : 0,
        lineWidth: (line as HTMLElement).getBoundingClientRect().width,
      };
    });
    expect(flow).not.toBeNull();
    expect(flow!.rowCount).toBeGreaterThan(1);
    expect(flow!.rowCount).toBeLessThan(5);
    expect(flow!.firstRowWidth).toBeGreaterThan(flow!.lineWidth * 0.8);

    // 3. A two-space hard break is a real line break: it stays its own line.
    const brokenLines = await page.evaluate(() =>
      Array.from(document.querySelectorAll('.cm-line')).filter((el) =>
        (el.textContent ?? '').includes('Broken here on purpose'),
      ).map((el) => el.textContent ?? ''),
    );
    expect(brokenLines).toHaveLength(1);
    expect(brokenLines[0]).not.toContain('Second visual line');
    await story.step(page, 'paragraph reflowed to the column');
    await expect(page.locator('.cm-editor')).toHaveScreenshot('editor-live-reflow.png');

    // 4. Typing at a join lands in the source line the caret sits on, in
    // order. A join range that reached back over the line's last character
    // swallowed each space as it was typed, which left the caret inside the
    // replaced range and wrote " indeed" as "deedni" across the break.
    const joinPoint = await page.evaluate(() => {
      const line = Array.from(document.querySelectorAll('.cm-line')).find((el) =>
        (el.textContent ?? '').startsWith('The quick brown fox'),
      );
      if (!line) return null;
      const walker = document.createTreeWalker(line, NodeFilter.SHOW_TEXT);
      let node: Node | null;
      while ((node = walker.nextNode())) {
        // The first join: the widget that stands in for the first line break.
        const parent = node.parentElement;
        if (!parent?.classList.contains('cm-lp-softbreak')) continue;
        const range = document.createRange();
        range.setStart(line, 0);
        range.setEndBefore(parent);
        const rects = Array.from(range.getClientRects()).filter((r) => r.width > 0);
        const last = rects[rects.length - 1];
        return last ? { x: last.right, y: last.top + last.height / 2 } : null;
      }
      return null;
    });
    expect(joinPoint).not.toBeNull();
    await page.mouse.click(joinPoint!.x, joinPoint!.y);
    await page.keyboard.type(' indeed');
    await expect(content).toContainText('jumps over indeed the lazy dog');

    // 5. Source mode gives the source lines back, one rendered line each.
    await page.getByRole('button', { name: 'Source', exact: true }).click();
    const sourceLines = await page.evaluate(() =>
      Array.from(document.querySelectorAll('.cm-line')).map((el) => el.textContent ?? ''),
    );
    expect(sourceLines).toContain('The quick brown fox jumps over indeed');
    expect(sourceLines).toContain('the lazy dog while the sleepy');
    await story.step(page, 'source mode keeps the source lines');
  });
  /**
   * The Properties card crosses two renderers — a CodeMirror block widget and
   * the reading view's markdown pipeline — and they used to disagree. A note
   * whose frontmatter defeated the flat parser showed eleven mono YAML lines
   * in the editor and NOTHING in the reading view. Every agent card carries a
   * nested `tools:` map, so this was not an edge case.
   *
   * The gate compares the two surfaces' own markup, not a screenshot of each:
   * a difference in the card is a difference in the string, and a baseline
   * would only catch the ones that also move a pixel.
   */
  const cardMarkup = (page: Page) =>
    page.getByTestId('fm-card').evaluate((el) => el.outerHTML);

  test('the Properties card reads the same in live preview and the reading view', async ({ page }, testInfo) => {
    const story = createStory(testInfo);
    const PROPS_NOTE = {
      name: 'Props Note',
      path: `${HARNESS_KILN}/Props Note.md`,
      content: [
        '---',
        'title: Coder',
        'description: >-',
        '  A folded scalar written over',
        '  two source lines.',
        'tags:',
        '  - agent',
        '  - coding',
        'tools:',
        '  semantic_search: true',
        '  grep_notes: true',
        '---',
        '',
        '# Coder Agent',
        '',
        'Body under the card.',
        '',
      ].join('\n'),
    };
    const harness = await setupEditorHarness(page, [PROPS_NOTE]);
    await harness.open(PROPS_NOTE);
    await expect(page.locator('.cm-editor')).toBeVisible({ timeout: 5000 });
    await page.locator('.cm-content').click();
    await page.keyboard.press('Control+End');

    // Live preview: the nested map becomes dotted rows, the folded scalar one
    // line, and no raw YAML reaches the screen.
    const card = page.getByTestId('fm-card');
    await expect(card).toBeVisible();
    await expect(card).toContainText('5 properties');
    await page.getByTestId('fm-summary').click();
    await expect(card).toContainText('tools.semantic_search');
    await expect(card).toContainText('A folded scalar written over two source lines.');
    await expect(page.locator('.cm-content')).not.toContainText('---');
    const live = await cardMarkup(page);
    await story.step(page, 'properties card in live preview');
    await expect(card).toHaveScreenshot('editor-live-properties.png');

    // Reading view: the same card, character for character.
    await page.getByRole('button', { name: 'Reading view', exact: true }).click();
    await expect(page.getByTestId('fm-card')).toBeVisible();
    await page.getByTestId('fm-summary').click();
    await expect(page.getByTestId('fm-card')).toContainText('tools.semantic_search');
    expect(await cardMarkup(page)).toBe(live);
    await story.step(page, 'the same card in the reading view');
  });

  /**
   * Pressing the card must reveal its rows, not redraw the block. The open
   * card used to become a bordered box: the hairline row turned into a tall
   * frame whose left border ran parallel to the pane separator a few pixels
   * away, and its `padding: 8px 12px` overrode the `padding-right` that
   * reserves the editor's toggle gutter, so the frame ran under the mode
   * buttons. Closed and open were two different shapes.
   */
  test('the Properties card keeps one shape open and closed', async ({ page }, testInfo) => {
    const story = createStory(testInfo);
    const SHAPE_NOTE = {
      name: 'Shape Note',
      path: `${HARNESS_KILN}/Shape Note.md`,
      content: ['---', 'title: Shape', 'status: draft', '---', '', '# Shape', '', 'Body.', ''].join('\n'),
    };
    const harness = await setupEditorHarness(page, [SHAPE_NOTE]);
    await harness.open(SHAPE_NOTE);
    await expect(page.locator('.cm-editor')).toBeVisible({ timeout: 5000 });
    await page.locator('.cm-content').click();
    await page.keyboard.press('Control+End');

    const summary = page.getByTestId('fm-summary');
    const card = page.getByTestId('fm-card');
    const shapeOf = async () => ({
      row: await summary.evaluate((el) => {
        const r = el.getBoundingClientRect();
        const cs = getComputedStyle(el);
        return { left: Math.round(r.left), right: Math.round(r.right), border: cs.borderTopWidth };
      }),
      // The card itself draws no frame in either state.
      cardBorder: await card.evaluate((el) => getComputedStyle(el).borderTopWidth),
    });

    const closed = await shapeOf();
    await summary.click();
    await expect(card).toHaveJSProperty('open', true);
    const open = await shapeOf();

    // Same row, same edges, same hairline — only the caret and the rows move.
    expect(open.row).toEqual(closed.row);
    expect(closed.row.border).toBe('1px');
    expect(open.cardBorder).toBe('0px');
    expect(closed.cardBorder).toBe('0px');

    // Mode controls occupy their own toolbar row, outside the note content.
    const toolbar = await page.getByRole('toolbar', { name: 'Note view' }).boundingBox();
    const editor = await page.locator('.cm-editor').boundingBox();
    expect(toolbar!.y + toolbar!.height).toBeLessThanOrEqual(editor!.y);
    await story.step(page, 'one shape, open and closed');
  });

  test('frontmatter beyond the parser gets the same raw card on both surfaces', async ({ page }, testInfo) => {
    const story = createStory(testInfo);
    const ODD_NOTE = {
      name: 'Odd Note',
      path: `${HARNESS_KILN}/Odd Note.md`,
      // A TOML inline table: beyond the flat parser on purpose.
      content: ['+++', 'point = { x = 1, y = 2 }', 'name = "odd"', '+++', '', '# Odd Body', ''].join('\n'),
    };
    const harness = await setupEditorHarness(page, [ODD_NOTE]);
    await harness.open(ODD_NOTE);
    await expect(page.locator('.cm-editor')).toBeVisible({ timeout: 5000 });
    await page.locator('.cm-content').click();
    await page.keyboard.press('Control+End');

    await expect(page.getByTestId('fm-card')).toContainText('raw properties');
    await page.getByTestId('fm-summary').click();
    await expect(page.getByTestId('fm-raw')).toContainText('point = { x = 1, y = 2 }');
    const live = await cardMarkup(page);
    await story.step(page, 'raw card in live preview');

    await page.getByRole('button', { name: 'Reading view', exact: true }).click();
    await expect(page.getByTestId('fm-card')).toBeVisible();
    await page.getByTestId('fm-summary').click();
    await expect(page.getByTestId('fm-raw')).toContainText('point = { x = 1, y = 2 }');
    expect(await cardMarkup(page)).toBe(live);
    await story.step(page, 'the same raw card in the reading view');

    // Live preview still edits: clicking the card drops the cursor into the
    // real source lines, which a card must never hide for good. The editor
    // remounts on the way back, so the card starts collapsed again.
    await page.getByRole('button', { name: 'Live preview', exact: true }).click();
    await expect(page.getByTestId('fm-card')).toBeVisible();
    await page.getByTestId('fm-summary').click();
    await page.getByTestId('fm-raw').click();
    await expect(page.locator('.cm-content')).toContainText('point = { x = 1, y = 2 }');
    await story.step(page, 'the raw card drops into its source');
  });
});
