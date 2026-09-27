import { test, expect } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';
import { appReady } from './helpers/nav';

test('WS-250: a saved base opens in its native panel and switches to kanban', async ({ page }) => {
  await setupBasicMocks(page, { kilns: { kilns: [{ name: 'Work', path: '/kiln', registered: true }], default_kiln: 'Work' } });
  const row = { path: 'First.md', ancestor_hash: 'hash', values: { 'file.name': { type: 'string', value: 'First.md' }, 'note.status': { type: 'string', value: 'todo' } } };
  await page.route('**/api/bases/query?**', route => {
    const board = new URL(route.request().url()).searchParams.get('view') === 'Board';
    return route.fulfill({ json: { root: '/kiln', source_hash: 'source', view: board ? 'Board' : 'Tasks', view_type: board ? 'kanban' : 'table', columns: [{ property: 'file.name', display_name: 'Name' }, { property: 'note.status', display_name: 'Status' }], rows: [row], groups: board ? [{ value: { type: 'string', value: 'todo' }, rows: [row], summaries: {} }, { value: { type: 'string', value: 'done' }, rows: [], summaries: {} }] : [], summaries: {}, group_property: board ? 'note.status' : null, views: [{ name: 'Tasks', type: 'table' }, { name: 'Board', type: 'kanban' }] } });
  });
  await page.goto('/');
  await appReady(page);
  await page.evaluate(async () => {
    const { openFileInEditor } = await import('/src/lib/file-actions.ts');
    openFileInEditor('/kiln/Tasks.base');
  });
  const base = page.getByRole('region', { name: 'Base view', exact: true });
  await expect(base.getByRole('columnheader', { name: 'Name' })).toBeVisible();
  await expect(base.getByText('First.md')).toBeVisible();
  await base.getByRole('combobox').selectOption('Board');
  await expect(base.getByRole('heading', { name: 'done 0' })).toBeVisible();
  await expect(base.getByRole('heading', { name: 'todo 1' })).toBeVisible();
  await page.screenshot({ path: '/tmp/crucible-bases-board.png', fullPage: true });
});
