import { test, expect } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';
import { MOCK_SESSION } from './helpers/fixtures';
import { openSession } from './helpers/nav';
import { historyOf, segment, userTurn } from '../src/test-utils/transcript';

test('user and assistant Copy work without navigator.clipboard', async ({ page }) => {
  await setupBasicMocks(page, { sessionHistory: historyOf(MOCK_SESSION.session_id, [
    userTurn('turn', 'Copy this question'), segment('turn', 0, 'Copy this answer\n\n```text\nCopy this code\n```'),
  ]) });
  await page.addInitScript(() => Object.defineProperty(navigator, 'clipboard', { value: undefined, configurable: true }));
  await page.goto('/');
  await openSession(page, MOCK_SESSION.session_id);
  const composer = page.getByRole('textbox', { name: 'Type a message...' });
  await page.getByTestId('message-user').hover();
  await page.getByRole('button', { name: 'Copy message', exact: true }).click();
  await composer.focus();
  await page.keyboard.press('Control+v');
  await expect(composer).toHaveValue('Copy this question');
  await composer.fill('');
  await page.getByRole('button', { name: 'Copy response', exact: true }).click();
  await composer.focus();
  await page.keyboard.press('Control+v');
  await expect(composer).toHaveValue('Copy this answer\n\n```text\nCopy this code\n```');
  await composer.fill('');
  const codeCopy = page.getByTestId('message-assistant').locator('[data-copy]');
  await codeCopy.hover();
  await codeCopy.click();
  await expect(codeCopy).toHaveText('Copied');
  await composer.focus();
  await page.keyboard.press('Control+v');
  await expect(composer).toHaveValue('Copy this code\n');
});
