import { test, expect } from '@playwright/test';
import { createStory } from './_helpers/story';
import { HARNESS_KILN, setupEditorHarness, typeInEditor } from './_helpers/editor-harness';

/**
 * Story: WS-203 — multiple open files as tabs; switching preserves per-file
 * content. Drives the real EditorPanel tab bar via the dev-only harness. See
 * editor-roundtrip.story.spec.ts for the editor's product-gap notes.
 *
 * History: bugs 5 (spurious dirty flag on the reused CodeMirror instance) and
 * 6 (dirty tab close silently discards) were pinned here as TODO(product)
 * tests until fixed on 2026-07-12. Programmatic doc swaps are now tagged with
 * the `contentSync` annotation so the update listener ignores them, and
 * closeFile() confirms before discarding a dirty file.
 */

const NOTE_A = { name: 'Note A', path: `${HARNESS_KILN}/Note A.md`, content: 'alpha content\n' };
const NOTE_B = { name: 'Note B', path: `${HARNESS_KILN}/Note B.md`, content: 'beta content\n' };

test.describe('WS-203 multi-file tabs', () => {
  test('switching tabs preserves each file’s content', async ({ page }, testInfo) => {
    const story = createStory(testInfo);
    const harness = await setupEditorHarness(page, [NOTE_A, NOTE_B]);

    await harness.open(NOTE_A);
    await expect(page.locator('.cm-content')).toContainText('alpha content');
    await harness.open(NOTE_B);
    await expect(page.locator('.cm-content')).toContainText('beta content');
    await expect(page.getByRole('button', { name: /Note A\.md/ })).toBeVisible();
    await expect(page.getByRole('button', { name: /Note B\.md/ })).toBeVisible();
    await story.step(page, 'two files open');

    // Append a marker to the active file (B).
    await typeInEditor(page, 'EDITED_B');
    await expect(page.locator('.cm-content')).toContainText('EDITED_B');
    await story.step(page, 'B edited');

    // Switch to A: A's content is intact, B's edit is not shown.
    await page.getByRole('button', { name: /Note A\.md/ }).click();
    await expect(page.locator('.cm-content')).toContainText('alpha content');
    await expect(page.locator('.cm-content')).not.toContainText('EDITED_B');
    await story.step(page, 'switched to A - content preserved');

    // Switch back to B: the unsaved edit is still there.
    await page.getByRole('button', { name: /Note B\.md/ }).click();
    await expect(page.locator('.cm-content')).toContainText('EDITED_B');
    await expect(page.locator('.cm-content')).toContainText('beta content');
    await story.step(page, 'switched back to B - edit intact');
  });

  test('opening a second file leaves both files clean (bug 5 regression)', async ({ page }) => {
    const harness = await setupEditorHarness(page, [NOTE_A, NOTE_B]);
    await harness.open(NOTE_A);
    await expect(page.getByRole('button', { name: /Note A\.md/ })).toBeVisible();
    // First file opens clean.
    await expect(page.getByText('●')).toHaveCount(0);

    await harness.open(NOTE_B);
    await expect(page.getByRole('button', { name: /Note B\.md/ })).toBeVisible();
    // The programmatic doc swap into the reused CodeMirror instance is not a
    // user edit — no dirty marker anywhere.
    await expect(page.getByText('●')).toHaveCount(0);
    expect(harness.saves).toHaveLength(0);
  });

  test('switching tabs does not mark the incoming file dirty (bug 5 regression)', async ({ page }) => {
    const harness = await setupEditorHarness(page, [NOTE_A, NOTE_B]);
    await harness.open(NOTE_A);
    await harness.open(NOTE_B);
    await expect(page.getByText('●')).toHaveCount(0);

    await page.getByRole('button', { name: /Note A\.md/ }).click();
    await expect(page.locator('.cm-content')).toContainText('alpha content');
    await expect(page.getByText('●')).toHaveCount(0);

    // A real edit still marks exactly the edited file dirty.
    await typeInEditor(page, 'REAL_EDIT');
    await expect(page.getByRole('button', { name: /●.*Note A\.md/ })).toBeVisible();
    await expect(page.getByText('●')).toHaveCount(1);
    expect(harness.saves).toHaveLength(0);
  });

  test('closing a dirty tab asks before discarding (bug 6 regression)', async ({ page }) => {
    const harness = await setupEditorHarness(page, [NOTE_A, NOTE_B]);
    await harness.open(NOTE_A);
    await harness.open(NOTE_B);
    await typeInEditor(page, 'UNSAVED');
    await expect(page.locator('.cm-content')).toContainText('UNSAVED');

    // Decline the confirm: the tab must survive with its edit intact.
    page.once('dialog', (dialog) => void dialog.dismiss());
    await page.getByRole('button', { name: /Note B\.md/ }).getByText('×').click();
    await expect(page.getByRole('button', { name: /Note B\.md/ })).toBeVisible();
    await expect(page.locator('.cm-content')).toContainText('UNSAVED');

    // Accept the confirm: now the tab closes and the edit is discarded.
    page.once('dialog', (dialog) => void dialog.accept());
    await page.getByRole('button', { name: /Note B\.md/ }).getByText('×').click();
    await expect(page.getByRole('button', { name: /Note B\.md/ })).toHaveCount(0);
    await expect(page.locator('.cm-content')).toContainText('alpha content');
    // Nothing was saved during the discard.
    expect(harness.saves).toHaveLength(0);
  });

  test('closing a clean tab never prompts', async ({ page }) => {
    const harness = await setupEditorHarness(page, [NOTE_A, NOTE_B]);
    await harness.open(NOTE_A);
    await harness.open(NOTE_B);
    await expect(page.getByText('●')).toHaveCount(0);

    let sawDialog = false;
    page.on('dialog', (dialog) => {
      sawDialog = true;
      return void dialog.accept();
    });
    await page.getByRole('button', { name: /Note B\.md/ }).getByText('×').click();
    await expect(page.getByRole('button', { name: /Note B\.md/ })).toHaveCount(0);
    expect(sawDialog).toBe(false);
    expect(harness.saves).toHaveLength(0);
  });
});
