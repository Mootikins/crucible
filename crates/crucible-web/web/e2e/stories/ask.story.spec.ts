import { test, expect, type Page } from '@playwright/test';
import { setupBasicMocks } from '../helpers/mock-api';
import { createStory } from './_helpers/story';

/**
 * Story: WS-105 — answer agent questions (Ask).
 *
 * The daemon emits an `interaction_requested` frame of kind `ask`; the real
 * AskInteraction renders single-select (radio), multi-select (checkbox), and
 * free-text variants inline. Submit POSTs /api/interaction/respond with
 * { selected: number[], other?: string }.
 *
 * DOCUMENTED GAP (WS-105 acceptance: "cancel sends a cancelled response"):
 * AskInteraction has NO cancel affordance — only a Submit button (disabled
 * until a selection/text exists). There is no way to cancel from the browser.
 * The variants below cover the answer paths that exist.
 */

function askFrame(
  id: string,
  fields: { question: string; choices?: string[]; multi_select?: boolean; allow_other?: boolean },
) {
  return {
    type: 'interaction_requested',
    data: { type: 'interaction_requested', kind: 'ask', id, ...fields },
  };
}

async function openAsk(page: Page, frame: { type: string; data: object }) {
  await setupBasicMocks(page, { sseEvents: [frame] });
  let respondBody: unknown = null;
  await page.route('**/api/interaction/respond', (route) => {
    respondBody = route.request().postDataJSON();
    return route.fulfill({ status: 200, body: '' });
  });
  await page.goto('/');
  await page.getByTestId('session-item-test-session-001').click();
  await expect(page.getByTestId('chat-input')).toBeEnabled({ timeout: 5000 });
  return { getRespond: () => respondBody };
}

test.describe('WS-105 Ask interactions', () => {
  test('single-select: pick one option', async ({ page }, testInfo) => {
    const story = createStory(testInfo);
    const ctx = await openAsk(
      page,
      askFrame('ask-1', { question: 'Pick a framework', choices: ['SolidJS', 'React', 'Svelte'] }),
    );

    await expect(page.getByText('Pick a framework')).toBeVisible({ timeout: 5000 });
    await story.step(page, 'single-select question');
    await page.getByRole('radio').nth(1).check(); // React

    const respondPromise = page.waitForRequest('**/api/interaction/respond');
    await page.getByRole('button', { name: 'Submit' }).click();
    await respondPromise;

    const body = ctx.getRespond() as { request_id: string; response: { selected: number[] } };
    expect(body.request_id).toBe('ask-1');
    expect(body.response.selected).toEqual([1]);
    await story.step(page, 'answered');
  });

  test('multi-select: pick several options', async ({ page }, testInfo) => {
    const story = createStory(testInfo);
    const ctx = await openAsk(
      page,
      askFrame('ask-2', {
        question: 'Which files?',
        choices: ['a.md', 'b.md', 'c.md'],
        multi_select: true,
      }),
    );

    await expect(page.getByText('Which files?')).toBeVisible({ timeout: 5000 });
    await page.getByRole('checkbox').nth(0).check();
    await page.getByRole('checkbox').nth(2).check();
    await story.step(page, 'multi-select two checked');

    const respondPromise = page.waitForRequest('**/api/interaction/respond');
    await page.getByRole('button', { name: 'Submit' }).click();
    await respondPromise;

    const body = ctx.getRespond() as { response: { selected: number[] } };
    expect(body.response.selected.sort()).toEqual([0, 2]);
  });

  test('free-text: type an answer', async ({ page }, testInfo) => {
    const story = createStory(testInfo);
    const ctx = await openAsk(page, askFrame('ask-3', { question: 'Name the branch?' }));

    await expect(page.getByText('Name the branch?')).toBeVisible({ timeout: 5000 });
    await page.getByPlaceholder('Type your answer...').fill('feat/web-tests');
    await story.step(page, 'free-text entered');

    const respondPromise = page.waitForRequest('**/api/interaction/respond');
    await page.getByRole('button', { name: 'Submit' }).click();
    await respondPromise;

    const body = ctx.getRespond() as { response: { selected: number[]; other?: string } };
    expect(body.response.other).toBe('feat/web-tests');
    expect(body.response.selected).toEqual([]);
    await story.step(page, 'answered');
  });

  test('choices + allow_other: pick "type your own"', async ({ page }) => {
    const ctx = await openAsk(
      page,
      askFrame('ask-4', {
        question: 'Pick or type',
        choices: ['one', 'two'],
        allow_other: true,
      }),
    );
    await expect(page.getByText('Pick or type')).toBeVisible({ timeout: 5000 });
    await page.getByPlaceholder('Or type your own...').fill('three');
    await page.getByRole('button', { name: 'Submit' }).click();
    await expect.poll(() => (ctx.getRespond() as { response?: { other?: string } })?.response?.other)
      .toBe('three');
  });
});
