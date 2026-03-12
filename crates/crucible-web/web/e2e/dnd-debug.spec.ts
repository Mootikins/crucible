import { test, expect, type Page } from '@playwright/test';

async function waitForApp(page: Page) {
  await page.route('**/api/layout', async (route) => {
    const method = route.request().method();
    if (method === 'GET') {
      await route.fulfill({ status: 404, contentType: 'application/json', body: '{}' });
      return;
    }
    if (method === 'POST' || method === 'DELETE') {
      await route.fulfill({ status: 200, contentType: 'application/json', body: '{}' });
      return;
    }
    await route.continue();
  });
  await page.goto('/');
  await page.waitForTimeout(500);
}

test('debug: edge drag to center - full analysis', async ({ page }) => {
  await waitForApp(page);

  const edgeTab = page.locator('[data-testid="edge-tab-left-search-tab"]');
  const edgeBox = await edgeTab.boundingBox();

  const viewport = page.viewportSize()!;
  const from = { x: edgeBox!.x + edgeBox!.width / 2, y: edgeBox!.y + edgeBox!.height / 2 };
  const to = { x: viewport.width * 0.6, y: viewport.height * 0.5 };

  await page.mouse.move(from.x, from.y);
  await page.mouse.down();
  await page.mouse.move(from.x + 5, from.y, { steps: 2 });
  await page.waitForTimeout(100);
  await page.mouse.move(to.x, to.y, { steps: 20 });
  await page.waitForTimeout(500);

  const overlayInfo = await page.evaluate(() => {
    const all = document.querySelectorAll('*');
    const fixed: any[] = [];
    all.forEach(el => {
      const style = getComputedStyle(el);
      if (style.position === 'fixed' && el.textContent?.trim()) {
        fixed.push({
          text: el.textContent.trim().substring(0, 30),
          rect: el.getBoundingClientRect().toJSON(),
          style: { top: style.top, left: style.left, transform: style.transform },
        });
      }
    });
    return fixed;
  });
  console.log('Overlay during edge->center drag:', JSON.stringify(overlayInfo, null, 2));

  const blueHighlights = await page.evaluate(() => {
    const els = document.querySelectorAll('[class*="bg-blue"]');
    return Array.from(els).filter(el => {
      const style = getComputedStyle(el);
      return style.display !== 'none' && (el as HTMLElement).offsetParent !== null;
    }).map(el => ({
      classes: el.className.substring(0, 120),
      rect: el.getBoundingClientRect().toJSON(),
    }));
  });
  console.log('Blue highlights:', JSON.stringify(blueHighlights, null, 2));

  await page.mouse.up();
  await page.waitForTimeout(300);

  const edgeVisible = await edgeTab.isVisible();
  const centerSearch = await page.locator('[data-tab-id="search-tab"]').isVisible();
  console.log('Result: edge visible =', edgeVisible, ', center search =', centerSearch);
  expect(true).toBe(true);
});

test('debug: center drag to edge - full analysis', async ({ page }) => {
  await waitForApp(page);

  const centerTab = page.locator('[data-tab-id="tab-chat-1"]');
  const centerBox = await centerTab.boundingBox();
  const edgeTabBar = page.locator('[data-testid="edge-tabbar-left"]');
  const edgeBarBox = await edgeTabBar.boundingBox();

  const from = { x: centerBox!.x + centerBox!.width / 2, y: centerBox!.y + centerBox!.height / 2 };
  const to = { x: edgeBarBox!.x + edgeBarBox!.width / 2, y: edgeBarBox!.y + edgeBarBox!.height / 2 };

  await page.mouse.move(from.x, from.y);
  await page.mouse.down();
  await page.mouse.move(from.x + 5, from.y, { steps: 2 });
  await page.waitForTimeout(100);
  await page.mouse.move(to.x, to.y, { steps: 20 });
  await page.waitForTimeout(500);

  const overlayInfo = await page.evaluate(() => {
    const all = document.querySelectorAll('*');
    const fixed: any[] = [];
    all.forEach(el => {
      const style = getComputedStyle(el);
      if (style.position === 'fixed' && el.textContent?.trim()) {
        fixed.push({
          text: el.textContent.trim().substring(0, 30),
          rect: el.getBoundingClientRect().toJSON(),
          style: { top: style.top, left: style.left, transform: style.transform },
        });
      }
    });
    return fixed;
  });
  console.log('Overlay during center->edge drag:', JSON.stringify(overlayInfo, null, 2));

  const blueHighlights = await page.evaluate(() => {
    const els = document.querySelectorAll('[class*="bg-blue"]');
    return Array.from(els).filter(el => {
      const style = getComputedStyle(el);
      return style.display !== 'none' && (el as HTMLElement).offsetParent !== null;
    }).map(el => ({
      classes: el.className.substring(0, 120),
      rect: el.getBoundingClientRect().toJSON(),
    }));
  });
  console.log('Blue highlights:', JSON.stringify(blueHighlights, null, 2));

  await page.mouse.up();
  await page.waitForTimeout(300);

  const edgeTab = page.locator('[data-testid="edge-tab-left-tab-chat-1"]');
  const edgeVisible = await edgeTab.isVisible();
  console.log('Result: edge tab-chat-1 visible =', edgeVisible);
  expect(true).toBe(true);
});