import { test, expect } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';
import { openSession } from './helpers/nav';
import { MOCK_SESSION } from './helpers/fixtures';

test('secondary panels share flat headers and folded Terminal can be restored', async ({ page }, info) => {
  await page.route('**/api/rpc/**', route => route.fulfill({ status: 404, json: { error: 'Unconfigured test fixture' } }));
  await setupBasicMocks(page);
  await page.route('**/api/rpc/surface.list', route => route.fulfill({ json: { surfaces: [] } }));
  await page.route('**/api/rpc/skills.list', route => route.fulfill({ json: { skills: [] } }));
  await page.goto('/');
  await expect(page.getByTestId('layout-menu')).toBeVisible();
  for (const [id, title] of [['backlinks', 'Backlinks'], ['activity', 'Activity'], ['skills', 'Skills'], ['plugins', 'Plugins'], ['changes', 'Changes'], ['surfaces', 'Surfaces']]) {
    await page.evaluate(async id => {
      const { openPanelTab } = await import('/src/lib/panel-actions.ts');
      openPanelTab(id);
    }, id);
    const heading = page.getByRole('heading', { name: title, exact: true });
    await expect(heading).toBeVisible();
    expect(await heading.evaluate(el => getComputedStyle(el.parentElement!).borderBottomWidth)).toBe('0px');
    await page.screenshot({ path: info.outputPath(`${id}.png`) });
  }
  await page.getByTestId('layout-menu').click();
  await page.getByTestId('layout-readd').hover();
  await page.getByTestId('layout-readd-terminal').click();
  const terminal = page.locator('.wm-ribbon-tab[title="Terminal"]');
  await expect(terminal).toHaveAttribute('data-highlighted', '');
  await terminal.click();
  await expect(terminal).not.toHaveAttribute('data-highlighted', '');
  await terminal.click();
  await expect(terminal).toHaveAttribute('data-highlighted', '');
});

test('rail joins follow every active tab on either side', async ({ page }, info) => {
  await setupBasicMocks(page);
  await page.goto('/');
  await expect(page.getByTestId('layout-menu')).toBeVisible();
  for (const side of ['left', 'right']) {
    await page.evaluate(async side => {
      const { windowActions, windowStore } = await import('/src/stores/windowStore.ts');
      const { collectPanes } = await import('/src/windowing/model/tree.ts');
      const panes = collectPanes(windowStore.edgePanels[side].layout);
      const pane = panes[panes.length - 1];
      windowActions.setPaneCollapsed(pane.id, false);
      for (const [index, contentType] of ['backlinks', 'activity', 'files'].entries()) {
        windowActions.addTab(pane.tabGroupId, { id: 'join-' + side + index, title: contentType, contentType });
      }
    }, side);
    const tabs = page.getByTestId('edge-host-' + side).locator('.wm-ribbon-trailing .wm-ribbon-tab');
    for (let index = 0; index < await tabs.count(); index++) {
      const tab = tabs.nth(index);
      await tab.click();
      await expect(tab).toHaveAttribute('data-highlighted', '');
      await expect.poll(() => tab.evaluate(el => {
        const pane = document.querySelector('.wm-pane[data-pane-id="' + (el as HTMLElement).dataset.ribbonPaneId + '"]')!;
        const a = el.getBoundingClientRect(), b = pane.getBoundingClientRect();
        const expected = Math.abs(a.top - b.top) <= 1 ? 'top' : Math.abs(a.bottom - b.bottom) <= 1 ? 'bottom' : undefined;
        return (el as HTMLElement).dataset.paneEdge === expected;
      })).toBe(true);
      const shape = await tab.evaluate(el => ({
        radius: getComputedStyle(el, '::before').borderStartStartRadius,
        feet: getComputedStyle(el, '::after').backgroundImage,
      }));
      expect(shape.radius).toBe('12px');
      expect(shape.feet.match(/radial-gradient/g)?.length).toBe(2);
      const host = page.getByTestId('edge-host-' + side);
      const cap = await host.locator('.wm-splitter').first().evaluate(el => ({
        upper: getComputedStyle(el, '::before').backgroundImage,
        lower: getComputedStyle(el, '::after').backgroundImage,
      }));
      expect(cap.upper).toContain('radial-gradient');
      expect(cap.lower).toContain('radial-gradient');
      await page.mouse.move(600, 350);
      await page.screenshot({ path: info.outputPath(side + '-tab-' + index + '.png') });
    }
  }
});

test('swapping a lone session stows the terminal-only rail and files reopen it', async ({ page }) => {
  await setupBasicMocks(page);
  await page.route('**/api/kiln/file?*', route => route.fulfill({ json: { content: '# Newly opened', content_hash: 'fixture' } }));
  await page.goto('/');
  await expect(page.getByTestId('layout-menu')).toBeVisible();
  await openSession(page, MOCK_SESSION.session_id);
  await expect(page.getByRole('textbox', { name: 'Type a message...' })).toBeVisible();
  await page.getByTestId('ribbon-cmd-swap-centre').click();
  await expect.poll(() => page.evaluate(async () => {
    const { windowStore } = await import('/src/stores/windowStore.ts');
    return windowStore.edgePanels.right.mode;
  })).toBe('strip');
  await page.evaluate(async () => {
    const { openFileInEditor } = await import('/src/lib/file-actions.ts');
    openFileInEditor('/home/user/project/new.md');
  });
  await expect(page.getByTestId('edge-host-right').locator('.note-editor')).toBeVisible();
  await expect.poll(() => page.evaluate(async () => {
    const { windowStore } = await import('/src/stores/windowStore.ts');
    return windowStore.edgePanels.right.mode;
  })).toBe('docked');
});

test('centre splitters preserve a symmetric gutter and resize both axes', async ({ page }) => {
  await setupBasicMocks(page);
  await page.goto('/');
  await expect(page.getByTestId('layout-menu')).toBeVisible();
  for (const direction of ['horizontal', 'vertical']) {
    await page.evaluate(async direction => {
      const { windowActions, windowStore } = await import('/src/stores/windowStore.ts');
      if (windowStore.layout.type !== 'pane') return;
      windowActions.addTab(windowStore.layout.tabGroupId!, { id: 'split-first', title: 'Search', contentType: 'search' });
      windowActions.splitPane(windowStore.layout.id, direction as 'horizontal' | 'vertical');
      if (windowStore.layout.type === 'split') windowActions.addTab(windowStore.layout.second.tabGroupId!, { id: 'split-second', title: 'Search', contentType: 'search' });
    }, direction);
    const splitter = page.getByTestId('centre-column').getByTestId('resize-splitter').last();
    const box = (await splitter.boundingBox())!;
    const gap = await splitter.evaluate(el => parseFloat(getComputedStyle(el).getPropertyValue('--mk-gap')));
    expect(direction === 'horizontal' ? box.width : box.height).toBe(gap);
    const x = box.x + box.width / 2, y = box.y + box.height / 2;
    await page.mouse.move(x, y);
    await page.mouse.down();
    await page.mouse.move(x + (direction === 'horizontal' ? 35 : 0), y + (direction === 'vertical' ? 35 : 0), { steps: 5 });
    await page.mouse.up();
    const moved = (await splitter.boundingBox())!;
    expect(direction === 'horizontal' ? moved.x - box.x : moved.y - box.y).toBeGreaterThan(20);
    if (direction === 'horizontal') {
      await page.goto('/');
      await expect(page.getByTestId('layout-menu')).toBeVisible();
    }
  }
});
