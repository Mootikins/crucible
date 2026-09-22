import { test, expect } from '@playwright/test';
import { createStory } from './_helpers/story';
import { setupBasicMocks } from '../helpers/mock-api';
import { appReady } from '../helpers/nav';

/**
 * Story: the user opens a proposal from the Inbox.
 *
 * A consolidation pass proposed a change to one note. The disk has no change
 * yet. The Inbox lists the proposal with its author, its title, its file
 * count and its line counts, and counts it in the header. A click opens the
 * diff pane of the proposal, with its decisions.
 */

const ID = '7a1c2f3e-0000-4000-8000-000000000001';
const KILN = '/home/user/kiln';

const PROPOSAL = {
  id: ID,
  author: { kind: 'plugin', name: 'consolidation' },
  title: 'Merge the two notes on retries',
  created_at: '2026-09-21T10:00:00Z',
  state: { kind: 'open' },
  writes: [
    {
      root: KILN,
      path: 'notes/retries.md',
      base: { kind: 'hash', hash: 'h0' },
      new_text: '# Retries\n\nBack off, then try again.\n',
    },
  ],
};

test.describe('Proposals in the Inbox', () => {
  test('opens a proposal from the Inbox', async ({ page }, testInfo) => {
    const story = createStory(testInfo);
    await setupBasicMocks(page, {
      proposals: [PROPOSAL],
      diffTexts: {
        'notes/retries.md': { base_text: '# Retries\n', current_text: PROPOSAL.writes[0].new_text },
      },
    });

    await page.goto('/');
    await appReady(page);

    // The palette opens every registered panel, the Inbox too.
    await page.keyboard.press('Control+p');
    await page.getByPlaceholder(/Run a command/).fill('Open Inbox');
    await page.getByText('Open Inbox', { exact: true }).click();

    const row = page.getByTestId(`inbox-proposal-${ID}`);
    await expect(row).toBeVisible();
    await expect(row).toContainText('Merge the two notes on retries');
    await expect(row).toContainText('consolidation');
    await expect(row).toContainText('1 file');
    await expect(row).toContainText('+2 −0');
    await expect(page.getByText(/1 pending/)).toBeVisible();
    await story.step(page, 'the inbox lists the proposal and counts it');

    await page.getByTestId(`inbox-proposal-open-${ID}`).click();

    await expect(page.getByTestId('proposal-bar')).toBeVisible();
    await expect(page.getByTestId(`diff-file-${KILN}:notes/retries.md`)).toBeVisible();
    await story.step(page, 'the diff pane shows the proposal');
  });
});
