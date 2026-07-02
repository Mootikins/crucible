import { test, expect } from '@playwright/test';
import { createStory } from './_helpers/story';
import { HARNESS_KILN, setupEditorHarness, typeInEditor } from './_helpers/editor-harness';

/**
 * Story: WS-203 — multiple open files as tabs; switching preserves per-file
 * content. Drives the real EditorPanel tab bar via the dev-only harness. See
 * editor-roundtrip.story.spec.ts for the editor's product-gap notes.
 *
 * PRODUCT BUG this story exposes (documented, NOT fixed per plan):
 *   EditorPanel renders a single reused <CodeMirrorEditor>; when the active
 *   file changes, its `content` prop changes and a createEffect re-dispatches
 *   the whole document. The view's updateListener treats that programmatic
 *   replacement as a user edit and calls updateFileContent(), so:
 *     (a) opening a SECOND file immediately marks it dirty (●), and
 *     (b) switching to a tab marks the file you switch TO dirty.
 *   Content is preserved correctly; only the dirty flag is wrong. The tests
 *   below pin this CURRENT behavior with TODO(product) markers.
 *   Fix sketch: gate the updateListener while doing programmatic doc swaps, or
 *   key one CodeMirror instance per file instead of reusing one.
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

  test('TODO(product): opening a second file spuriously marks it dirty', async ({ page }) => {
    const harness = await setupEditorHarness(page, [NOTE_A, NOTE_B]);
    await harness.open(NOTE_A);
    await expect(page.getByRole('button', { name: /Note A\.md/ })).toBeVisible();
    // First file opens clean.
    await expect(page.getByText('●')).toHaveCount(0);

    await harness.open(NOTE_B);
    await expect(page.getByRole('button', { name: /Note B\.md/ })).toBeVisible();
    // BUG: B is dirty immediately, with zero edits. Should be 0.
    await expect(page.getByText('●')).toHaveCount(1);
    // The dirty one is B (the just-opened, just-activated file).
    await expect(page.getByRole('button', { name: /●.*Note B\.md/ })).toBeVisible();
    // No save was ever issued — the dirtiness is purely a UI-state artifact.
    expect(harness.saves).toHaveLength(0);
  });

  test('TODO(product): switching tabs marks the incoming file dirty', async ({ page }) => {
    const harness = await setupEditorHarness(page, [NOTE_A, NOTE_B]);
    await harness.open(NOTE_A);
    await harness.open(NOTE_B); // B now (spuriously) dirty → 1 marker
    await expect(page.getByText('●')).toHaveCount(1);

    // Switch to A. BUG: A becomes dirty too → 2 markers.
    await page.getByRole('button', { name: /Note A\.md/ }).click();
    await expect(page.getByText('●')).toHaveCount(2);
    expect(harness.saves).toHaveLength(0);
  });

  test('closing a dirty tab discards without a warning (WS-203 gap)', async ({ page }) => {
    const harness = await setupEditorHarness(page, [NOTE_A, NOTE_B]);
    await harness.open(NOTE_A);
    await harness.open(NOTE_B);
    await typeInEditor(page, 'UNSAVED');
    await expect(page.locator('.cm-content')).toContainText('UNSAVED');

    // Close the active (dirty) tab via its "×" affordance.
    await page.getByRole('button', { name: /Note B\.md/ }).getByText('×').click();

    // TODO(product): WS-203 wants a confirm prompt before discarding unsaved
    // work. There is none — closeFile() just splices the tab. Pin that:
    await expect(page.getByRole('button', { name: /Note B\.md/ })).toHaveCount(0);
    await expect(page.locator('.cm-content')).toContainText('alpha content');
    // Nothing was saved during the discard.
    expect(harness.saves).toHaveLength(0);
  });
});
