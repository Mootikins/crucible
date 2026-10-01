import { test, expect } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';
import { MOCK_SESSION } from './helpers/fixtures';
import { openSession } from './helpers/nav';
import { historyOf, segment, toolCard, userTurn } from '../src/test-utils/transcript';
test('tool rows match reference geometry and thoughts have balanced gaps', async ({ page }, info) => {
  await setupBasicMocks(page, { sessionHistory: historyOf(MOCK_SESSION.session_id, [
    userTurn('turn', 'Inspect the source'),
    toolCard('turn', 'before', { name: 'exec_command', display: { kind: 'command', tool: 'exec_command', command: 'pwd', render: { line: 'pwd' } } }),
    segment('turn', 0, 'Here is the result.', { thinking: 'Check the source carefully.' }),
    toolCard('turn', 'after', { name: 'exec_command', display: { kind: 'search', tool: 'exec_command', query: 'ModeDecl', render: { line: 'ModeDecl' } } }),
  ]) });
  await page.goto('/');
  await openSession(page, MOCK_SESSION.session_id);
  const before = page.locator('[data-tool-call-id="before"]');
  const thought = page.getByRole('button', { name: /^Thought/ });
  const prose = page.getByTestId('message-assistant').locator('.prose');
  await expect(thought).toBeVisible();
  await expect(thought.locator('.lucide-brain')).toBeVisible();
  const thoughtCaret = await thought.locator('.lucide-chevron-right').boundingBox();
  const toolCaret = await before.locator('.lucide-chevron-right').boundingBox();
  expect(Math.abs(thoughtCaret!.x - toolCaret!.x)).toBeLessThanOrEqual(1);
  const a = (await before.boundingBox())!;
  const b = (await thought.boundingBox())!;
  const c = (await prose.boundingBox())!;
  expect(Math.abs((b.y - a.y - a.height) - (c.y - b.y - b.height))).toBeLessThanOrEqual(1);
  await expect(before).toContainText('Ran');
  const geometry = await before.evaluate(el => ({ height: el.getBoundingClientRect().height, font: getComputedStyle(el.querySelector('button')!).fontFamily }));
  expect(geometry.height).toBe(26);
  expect(geometry.font).not.toMatch(/mono/i);
  await page.screenshot({ path: info.outputPath('transcript-tools.png') });
});
