import { test, expect } from '@playwright/test';

type ZoneAction = 
  | { type: 'toggle'; zone: 'left' | 'right' | 'bottom' }
  | { type: 'drag'; from: string; to: string };

function generateRandomActions(count: number, seed: number): ZoneAction[] {
  const zones: ('left' | 'right' | 'bottom')[] = ['left', 'right', 'bottom'];
  const tabs = ['Sessions', 'Files', 'Editor', 'Chat', 'Terminal'];
  const actions: ZoneAction[] = [];
  
  let rng = seed;
  const random = () => {
    rng = (rng * 1103515245 + 12345) & 0x7fffffff;
    return rng / 0x7fffffff;
  };

  for (let i = 0; i < count; i++) {
    if (random() < 0.7) {
      actions.push({
        type: 'toggle',
        zone: zones[Math.floor(random() * zones.length)],
      });
    } else {
      const fromIdx = Math.floor(random() * tabs.length);
      let toIdx = Math.floor(random() * tabs.length);
      if (toIdx === fromIdx) toIdx = (toIdx + 1) % tabs.length;
      actions.push({
        type: 'drag',
        from: tabs[fromIdx],
        to: tabs[toIdx],
      });
    }
  }
  return actions;
}

test.describe('Zone Stability', () => {
  test.beforeEach(async ({ page }) => {
    await page.addInitScript(() => {
      localStorage.clear();
    });
  });

  test('random toggle sequences do not cause errors', async ({ page }) => {
    const errors: string[] = [];
    page.on('console', msg => {
      if (msg.type() === 'error') {
        errors.push(msg.text());
      }
    });
    page.on('pageerror', err => errors.push(err.message));

    await page.goto('http://localhost:5173');
    await page.waitForTimeout(2000);

    const leftToggle = page.locator('[data-testid="toggle-left"]');
    const rightToggle = page.locator('[data-testid="toggle-right"]');
    const bottomToggle = page.locator('[data-testid="toggle-bottom"]');

    await expect(leftToggle).toBeVisible();
    await expect(rightToggle).toBeVisible();
    await expect(bottomToggle).toBeVisible();

    const actions = generateRandomActions(20, Date.now());
    console.log('Testing sequence:', actions.map(a => 
      a.type === 'toggle' ? `toggle-${a.zone}` : `drag-${a.from}-to-${a.to}`
    ).join(' -> '));

    for (const action of actions) {
      if (action.type === 'toggle') {
        const toggle = action.zone === 'left' ? leftToggle : 
                       action.zone === 'right' ? rightToggle : bottomToggle;
        await toggle.click();
        await page.waitForTimeout(100);
      }
    }

    const criticalErrors = errors.filter(e => 
      !e.includes('HTTP 500') && 
      !e.includes('Failed to') &&
      !e.includes('net::')
    );

    expect(criticalErrors).toEqual([]);
  });

  test('toggle sequence: left-right-left-right preserves state', async ({ page }) => {
    await page.goto('http://localhost:5173');
    await page.waitForTimeout(2000);

    const leftToggle = page.locator('[data-testid="toggle-left"]');
    const rightToggle = page.locator('[data-testid="toggle-right"]');
    const sessionsTab = page.locator('.dv-tab:has-text("Sessions")');

    await expect(sessionsTab).toBeVisible({ timeout: 5000 });

    const getLeftWidth = async () => {
      return await sessionsTab.evaluate(el => {
        const group = el.closest('.dv-groupview');
        return group ? group.getBoundingClientRect().width : -1;
      });
    };

    const initialWidth = await getLeftWidth();
    expect(initialWidth).toBeGreaterThan(50);

    await leftToggle.click();
    await page.waitForTimeout(200);
    expect(await getLeftWidth()).toBeLessThan(50);

    await rightToggle.click();
    await page.waitForTimeout(200);

    await leftToggle.click();
    await page.waitForTimeout(200);
    expect(await getLeftWidth()).toBeGreaterThan(50);

    await rightToggle.click();
    await page.waitForTimeout(200);

    expect(await getLeftWidth()).toBeGreaterThan(50);
  });

  test('toggle same zone multiple times is idempotent', async ({ page }) => {
    await page.goto('http://localhost:5173');
    await page.waitForTimeout(2000);

    const leftToggle = page.locator('[data-testid="toggle-left"]');
    const sessionsTab = page.locator('.dv-tab:has-text("Sessions")');

    await expect(sessionsTab).toBeVisible({ timeout: 5000 });

    const getGroupWidth = async () => {
      return await sessionsTab.evaluate(el => {
        const group = el.closest('.dv-groupview');
        return group ? group.getBoundingClientRect().width : -1;
      });
    };

    const initialWidth = await getGroupWidth();
    expect(initialWidth).toBeGreaterThan(50);

    await leftToggle.click();
    await page.waitForTimeout(300);
    const afterFirst = await getGroupWidth();

    await leftToggle.click();
    await page.waitForTimeout(300);
    const afterSecond = await getGroupWidth();

    await leftToggle.click();
    await page.waitForTimeout(300);
    const afterThird = await getGroupWidth();

    expect(afterFirst).toBeLessThan(50);
    expect(afterSecond).toBeGreaterThan(50);
    expect(afterThird).toBeLessThan(50);
  });

  test('all zones can be collapsed and expanded independently', async ({ page }) => {
    await page.goto('http://localhost:5173');
    await page.waitForTimeout(2000);

    const leftToggle = page.locator('[data-testid="toggle-left"]');
    const rightToggle = page.locator('[data-testid="toggle-right"]');

    const sessionsTab = page.locator('.dv-tab:has-text("Sessions")');
    const chatTab = page.locator('.dv-tab:has-text("Chat")');

    await expect(sessionsTab).toBeVisible({ timeout: 5000 });
    await expect(chatTab).toBeVisible({ timeout: 5000 });

    const getWidth = async (tab: typeof sessionsTab) => {
      return await tab.evaluate(el => {
        const group = el.closest('.dv-groupview');
        return group ? group.getBoundingClientRect().width : -1;
      });
    };

    const leftInitial = await getWidth(sessionsTab);
    const rightInitial = await getWidth(chatTab);
    expect(leftInitial).toBeGreaterThan(50);
    expect(rightInitial).toBeGreaterThan(50);

    await leftToggle.click();
    await page.waitForTimeout(200);
    expect(await getWidth(sessionsTab)).toBeLessThan(50);
    expect(await getWidth(chatTab)).toBeGreaterThan(50);

    await rightToggle.click();
    await page.waitForTimeout(200);
    expect(await getWidth(sessionsTab)).toBeLessThan(50);
    expect(await getWidth(chatTab)).toBeLessThan(50);

    await leftToggle.click();
    await page.waitForTimeout(200);
    expect(await getWidth(sessionsTab)).toBeGreaterThan(50);
    expect(await getWidth(chatTab)).toBeLessThan(50);

    await rightToggle.click();
    await page.waitForTimeout(200);
    expect(await getWidth(sessionsTab)).toBeGreaterThan(50);
    expect(await getWidth(chatTab)).toBeGreaterThan(50);
  });
});
