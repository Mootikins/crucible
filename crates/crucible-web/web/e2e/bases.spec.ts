import { test, expect } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';
import { appReady } from './helpers/nav';

test('WS-250: a saved base opens in its native panel and switches to kanban', async ({ page }) => {
  await setupBasicMocks(page, { kilns: { kilns: [{ name: 'Work', path: '/kiln', registered: true }], default_kiln: 'Work' } });
  const row = { path: 'First.md', ancestor_hash: 'hash', movable: true, values: { 'file.name': { type: 'string', value: 'First.md' }, 'note.status': { type: 'string', value: 'todo' } } };
  await page.route('**/api/bases/query?**', route => {
    const board = new URL(route.request().url()).searchParams.get('view') === 'Board';
    return route.fulfill({ json: { root: '/kiln', source_hash: 'source', view: board ? 'Board' : 'Tasks', view_type: board ? 'kanban' : 'table', columns: [{ property: 'file.name', display_name: 'Name' }, { property: 'note.status', display_name: 'Status' }], rows: [row], groups: board ? [{ value: { type: 'string', value: 'todo' }, write_value: 'todo', rows: [row], summaries: {} }, { value: { type: 'string', value: 'done' }, write_value: 'done', rows: [], summaries: {} }] : [], summaries: {}, options: { card_size: 200, column_width: 280, image: null, image_fit: 'cover', image_aspect_ratio: board ? 0.5 : 1, hide_empty_groups: false, markers: 'bullet', indent_properties: false, separator: ', ', row_height: 'short', column_size: {} }, group_property: board ? 'note.status' : null, views: [{ name: 'Tasks', type: 'table' }, { name: 'Board', type: 'kanban' }] } });
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

test('WS-253: native view options and typed values survive narrow layouts', async ({ page }) => {
  await setupBasicMocks(page, { kilns: { kilns: [{ name: 'Work', path: '/kiln', registered: true }], default_kiln: 'Work' } });
  await page.route('**/api/file/raw?**', route => route.fulfill({contentType:'image/svg+xml',body:'<svg xmlns="http://www.w3.org/2000/svg" width="120" height="80"><rect width="120" height="80" fill="#447799"/></svg>'}));
  await page.route('**/api/bases/query?**', route => {
    const name = new URL(route.request().url()).searchParams.get('view') ?? 'Cards';
    const row = {path:'A.md',ancestor_hash:'h',movable:false,values:{'file.name':{type:'string',value:'A 🦀'},done:{type:'boolean',value:true},labels:{type:'list',value:[{type:'string',value:'one'},{type:'string',value:'two'}]},cover:{type:'image',value:'cover.svg'},symbol:{type:'icon',value:'plus'},markup:{type:'html',value:'<b>Safe text</b><script>alert(1)</script>'}}};
    return route.fulfill({json:{root:'/kiln',source_hash:'h',source_path:'Tasks.base',view:name,view_type:name.toLowerCase(),options:{card_size:210,column_width:330,image:'cover',image_fit:'contain',image_aspect_ratio:1.5,hide_empty_groups:false,markers:'number',indent_properties:true,separator:' | ',row_height:'tall',column_size:{'file.name':180}},columns:[{property:'file.name',display_name:'Name'},{property:'done',display_name:'Done'},{property:'labels',display_name:'Labels'},{property:'symbol',display_name:'Symbol'},{property:'markup',display_name:'Markup'}],rows:[row],groups:[],summaries:{done:{type:'number',value:1}},views:['Cards','List','Table'].map(name=>({name,type:name.toLowerCase()}))}});
  });
  await page.goto('/'); await appReady(page);
  await page.evaluate(async () => { const {openFileInEditor}=await import('/src/lib/file-actions.ts'); openFileInEditor('/kiln/Tasks.base'); });
  const base = page.getByRole('region',{name:'Base view',exact:true});
  await page.setViewportSize({width:700,height:850});
  await expect(base.getByRole('img',{name:'Entry image'})).toHaveCSS('aspect-ratio','1.5 / 1');
  await expect.poll(() => base.getByRole('img',{name:'Entry image'}).evaluate((image: HTMLImageElement) => image.naturalWidth)).toBe(120);
  await expect(base.getByRole('checkbox')).toBeChecked();
  await expect(base.locator('svg[aria-label="plus"]')).toBeVisible();
  await expect(base.getByText('Safe text')).toBeVisible();
  await expect(base.locator('script')).toHaveCount(0);
  await page.screenshot({path:'/tmp/crucible-bases-cards-options.png',fullPage:true});
  await base.getByRole('combobox').selectOption('List');
  await expect(base.locator('ol')).toHaveCSS('list-style-type','decimal');
  await expect(base.locator('[data-base-properties]')).toHaveCSS('display','block');
  await page.screenshot({path:'/tmp/crucible-bases-list-options.png',fullPage:true});
  await base.getByRole('combobox').selectOption('Table');
  await expect(base.locator('td').first()).toHaveCSS('height','112px');
  await expect(base.getByRole('columnheader',{name:'Name'})).toHaveCSS('min-width','180px');
  await page.screenshot({path:'/tmp/crucible-bases-table-options.png',fullPage:true});
});
