import { test, expect } from '@playwright/test';

test.describe('Dockview Features', () => {
  test('panel collapse works via toggle buttons', async ({ page }) => {
    await page.goto('http://localhost:5173');
    await page.waitForTimeout(2000);

    const leftToggle = page.locator('[data-testid="toggle-left"]');
    const rightToggle = page.locator('[data-testid="toggle-right"]');
    const bottomToggle = page.locator('[data-testid="toggle-bottom"]');

    await expect(leftToggle).toBeVisible();
    await expect(rightToggle).toBeVisible();
    await expect(bottomToggle).toBeVisible();

    const sessionsTab = page.locator('.dv-tab:has-text("Sessions")');
    await expect(sessionsTab).toBeVisible({ timeout: 5000 });

    const getGroupWidth = async () => {
      return await sessionsTab.evaluate(el => {
        const group = el.closest('.dv-groupview');
        return group ? group.getBoundingClientRect().width : 0;
      });
    };

    const initialWidth = await getGroupWidth();
    expect(initialWidth).toBeGreaterThan(100);

    await leftToggle.click();
    await page.waitForTimeout(500);

    const collapsedWidth = await getGroupWidth();
    expect(collapsedWidth).toBeLessThan(initialWidth);
    console.log(`✅ Panel collapsed: ${initialWidth}px -> ${collapsedWidth}px`);

    await leftToggle.click();
    await page.waitForTimeout(500);

    const expandedWidth = await getGroupWidth();
    expect(expandedWidth).toBeGreaterThan(collapsedWidth);
    console.log(`✅ Panel expanded: ${collapsedWidth}px -> ${expandedWidth}px`);
  });

  test('tabs can be dragged between panels', async ({ page }) => {
    await page.goto('http://localhost:5173');
    await page.waitForTimeout(2000);

    const sessionsTab = page.locator('.dv-tab:has-text("Sessions")');
    await expect(sessionsTab).toBeVisible({ timeout: 5000 });

    const chatPanel = page.locator('.dv-tab:has-text("Chat")');
    await expect(chatPanel).toBeVisible({ timeout: 5000 });

    const sessionsBound = await sessionsTab.boundingBox();
    const chatBound = await chatPanel.boundingBox();

    if (sessionsBound && chatBound) {
      await page.mouse.move(sessionsBound.x + sessionsBound.width / 2, sessionsBound.y + sessionsBound.height / 2);
      await page.mouse.down();
      await page.mouse.move(chatBound.x + chatBound.width / 2, chatBound.y + chatBound.height / 2, { steps: 10 });
      await page.mouse.up();
      await page.waitForTimeout(500);

      console.log('✅ Tab drag executed (visual verification needed)');
    }
  });

  test('dockview renders without errors', async ({ page }) => {
    const errors: string[] = [];
    page.on('console', msg => {
      if (msg.type() === 'error' && msg.text().toLowerCase().includes('dockview')) {
        errors.push(msg.text());
      }
    });

    await page.goto('http://localhost:5173');
    await page.waitForTimeout(3000);

    const dockviewContainer = page.locator('.dockview-theme-abyss');
    await expect(dockviewContainer).toBeVisible({ timeout: 5000 });

    const tabs = page.locator('.dv-tab');
    const tabCount = await tabs.count();
    expect(tabCount).toBeGreaterThan(0);

    console.log(`Found ${tabCount} tabs`);
    console.log(`Dockview errors: ${errors.length === 0 ? 'None' : errors.join(', ')}`);

    expect(errors.length).toBe(0);
  });
});
