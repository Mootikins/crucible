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
    toolCard('turn', 'file', { name: 'read_file', display: { kind: 'file_read', tool: 'read_file', paths: ['/notes/example.md'] } }),
  ]) });
  await page.goto('/');
  await openSession(page, MOCK_SESSION.session_id);
  const before = page.locator('[data-tool-call-id="before"]');
  const thought = page.getByRole('button', { name: /^Thought/ });
  const prose = page.getByTestId('message-assistant').locator('.prose');
  await expect(thought).toBeVisible();
  await expect(thought.locator('.lucide-brain')).toBeVisible();
  const toolCaret = await before.locator('.lucide-chevron-right').boundingBox();
  const toggleBox = (await before.locator('.tool-call-toggle').boundingBox())!;
  expect(toolCaret!.x - toggleBox.x - toggleBox.width).toBeGreaterThanOrEqual(6);
  expect(toolCaret!.x - toggleBox.x - toggleBox.width).toBeLessThanOrEqual(8);
  const a = (await before.boundingBox())!;
  const b = (await thought.boundingBox())!;
  const c = (await prose.boundingBox())!;
  expect(Math.abs((b.y - a.y - a.height) - (c.y - b.y - b.height))).toBeLessThanOrEqual(1);
  await expect(before).toContainText('Ran');
  const geometry = await before.evaluate(el => ({ height: el.getBoundingClientRect().height, font: getComputedStyle(el.querySelector('button')!).fontFamily }));
  expect(geometry.height).toBe(26);
  expect(geometry.font).not.toMatch(/mono/i);
  await page.screenshot({ path: info.outputPath('transcript-tools.png') });
  const row = before.locator('.tool-call-row');
  const rowBox = (await row.boundingBox())!;
  const groupBox = (await before.locator('..').boundingBox())!;
  expect(rowBox.width).toBeLessThan(groupBox.width / 2);
  const toggle = before.locator('.tool-call-toggle');
  await row.click({ position: { x: rowBox.width - 2, y: rowBox.height / 2 } });
  await expect(toggle).toHaveAttribute('aria-expanded', 'true');
  await row.click({ position: { x: rowBox.width - 2, y: rowBox.height / 2 } });
  await expect(toggle).toHaveAttribute('aria-expanded', 'false');
  const file = page.getByRole('button', { name: 'Open /notes/example.md', exact: true });
  await file.locator('..').evaluate(el => (el as HTMLElement).style.maxWidth = '120px');
  const fileBox = (await file.boundingBox())!;
  const labelBox = (await file.locator('..').locator('.tool-call-toggle > span').first().boundingBox())!;
  expect(fileBox.x - labelBox.x - labelBox.width).toBeGreaterThanOrEqual(6);
  expect(await file.evaluate(element => {
    const box = element.getBoundingClientRect();
    return element.contains(document.elementFromPoint(box.x + box.width / 2, box.y + box.height / 2));
  })).toBe(true);
});

test('kiln-relative tool paths resolve before reading the editor file', async ({ page }) => {
  const path = '/home/user/notes/Search & Discovery.md';
  await setupBasicMocks(page, { sessionHistory: historyOf(MOCK_SESSION.session_id, [
    userTurn('turn', 'Read a note'),
    toolCard('turn', 'note', { name: 'read_note', display: { kind: 'file_read', tool: 'read_note', paths: ['Search & Discovery.md'] } }),
  ]) });
  await page.route('**/api/notes/resolve?*', async route => {
    const url = new URL(route.request().url());
    expect(url.searchParams.get('kiln')).toBe('/home/user/notes');
    expect(url.searchParams.get('name')).toBe('Search & Discovery.md');
    await route.fulfill({ json: { path: 'Search & Discovery.md', absolutePath: path, title: 'Search & Discovery' } });
  });
  const reads: string[] = [];
  await page.route('**/api/kiln/file?*', async route => {
    reads.push(new URL(route.request().url()).searchParams.get('path')!);
    await route.fulfill({ json: { content: '# Resolved note', content_hash: 'fixture' } });
  });
  await page.goto('/');
  await openSession(page, MOCK_SESSION.session_id);
  await page.getByRole('button', { name: 'Open Search & Discovery.md', exact: true }).click();
  await expect.poll(() => reads).toContain(path);
  expect(reads).not.toContain('Search & Discovery.md');
  await expect(page.getByText('Failed to read file:', { exact: false })).toHaveCount(0);
});
