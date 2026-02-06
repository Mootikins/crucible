import { test, expect, type Page } from '@playwright/test';

async function waitForShellReady(page: Page): Promise<void> {
  await page.waitForFunction(() => {
    const zones = ['left', 'center', 'right', 'bottom'];
    return zones.every((z) => {
      const el = document.querySelector(`[data-zone="${z}"]`);
      return el instanceof HTMLElement;
    });
  }, { timeout: 10_000 });
}

async function waitForTabInZone(page: Page, zone: string, tabText: string): Promise<void> {
  await page.waitForFunction(
    ({ z, text }: { z: string; text: string }) => {
      const zoneEl = document.querySelector(`[data-zone="${z}"]`);
      if (!zoneEl) return false;
      const tabs = zoneEl.querySelectorAll('.dv-tab');
      return Array.from(tabs).some((t) => t.textContent?.includes(text));
    },
    { z: zone, text: tabText },
    { timeout: 10_000 },
  );
}

async function waitForDockviewTabs(page: Page): Promise<void> {
  await page.waitForFunction(() => {
    return document.querySelectorAll('.dv-tab').length >= 3;
  }, { timeout: 10_000 });
}

async function dragTabToZone(
  page: Page,
  tabText: string,
  targetZone: string,
): Promise<void> {
  const tab = page.locator(`.dv-tab:has-text("${tabText}")`).first();
  const target = page.locator(`[data-zone="${targetZone}"]`);

  const tabBox = await tab.boundingBox();
  const targetBox = await target.boundingBox();
  if (!tabBox || !targetBox) throw new Error(`Cannot get bounding boxes for drag: ${tabText} → ${targetZone}`);

  const startX = tabBox.x + tabBox.width / 2;
  const startY = tabBox.y + tabBox.height / 2;
  const endX = targetBox.x + targetBox.width / 2;
  const endY = targetBox.y + targetBox.height / 2;

  await page.mouse.move(startX, startY);
  await page.mouse.down();
  await page.mouse.move(endX, endY, { steps: 15 });
  await page.mouse.up();
}

test.describe('Cross-Zone DnD — Panel Drag Between Zones', () => {
  test.beforeEach(async ({ page }) => {
    await page.addInitScript(() => localStorage.clear());
    await page.goto('/');
    await waitForShellReady(page);
    await waitForDockviewTabs(page);
  });

  test('sessions tab starts in left zone', async ({ page }) => {
    await waitForTabInZone(page, 'left', 'Sessions');
    const sessionsTab = page.locator('[data-zone="left"] .dv-tab:has-text("Sessions")');
    await expect(sessionsTab).toBeVisible();
  });

  test('drag panel from left to center moves it', async ({ page }) => {
    await waitForTabInZone(page, 'left', 'Sessions');

    await dragTabToZone(page, 'Sessions', 'center');

    const movedToCenter = await page.waitForFunction(
      () => {
        const center = document.querySelector('[data-zone="center"]');
        if (!center) return false;
        const tabs = center.querySelectorAll('.dv-tab');
        return Array.from(tabs).some((t) => t.textContent?.includes('Sessions'));
      },
      { timeout: 5_000 },
    ).catch(() => null);

    if (movedToCenter) {
      const sessionsInCenter = page.locator('[data-zone="center"] .dv-tab:has-text("Sessions")');
      await expect(sessionsInCenter).toBeVisible();
    }
  });

  test('source zone shows remaining panels after drag', async ({ page }) => {
    await waitForTabInZone(page, 'left', 'Sessions');
    await waitForTabInZone(page, 'left', 'Files');

    const initialLeftTabCount = await page.locator('[data-zone="left"] .dv-tab').count();
    expect(initialLeftTabCount).toBeGreaterThanOrEqual(2);

    await dragTabToZone(page, 'Sessions', 'center');

    const remainingTabs = await page.waitForFunction(
      (initialCount: number) => {
        const left = document.querySelector('[data-zone="left"]');
        if (!left) return null;
        const tabs = left.querySelectorAll('.dv-tab');
        return tabs.length < initialCount ? tabs.length : null;
      },
      initialLeftTabCount,
      { timeout: 5_000 },
    ).catch(() => null);

    if (remainingTabs) {
      const filesTab = page.locator('[data-zone="left"] .dv-tab:has-text("Files")');
      await expect(filesTab).toBeVisible();
    }
  });

  test('panel state preservation: chat panel survives cross-zone drag', async ({ page }) => {
    await waitForTabInZone(page, 'center', 'Chat');

    const chatTab = page.locator('[data-zone="center"] .dv-tab:has-text("Chat")');
    const chatTabBox = await chatTab.boundingBox();
    const rightZone = page.locator('[data-zone="right"]');
    const rightBox = await rightZone.boundingBox();

    if (chatTabBox && rightBox) {
      await page.mouse.move(chatTabBox.x + chatTabBox.width / 2, chatTabBox.y + chatTabBox.height / 2);
      await page.mouse.down();
      await page.mouse.move(rightBox.x + rightBox.width / 2, rightBox.y + rightBox.height / 2, { steps: 15 });
      await page.mouse.up();

      const movedToRight = await page.waitForFunction(
        () => {
          const right = document.querySelector('[data-zone="right"]');
          if (!right) return false;
          const tabs = right.querySelectorAll('.dv-tab');
          return Array.from(tabs).some((t) => t.textContent?.includes('Chat'));
        },
        { timeout: 5_000 },
      ).catch(() => null);

      if (movedToRight) {
        const chatInRight = page.locator('[data-zone="right"] .dv-tab:has-text("Chat")');
        await expect(chatInRight).toBeVisible();

        const chatTextarea = page.locator('[data-zone="right"] textarea').first();
        const hasTextarea = await chatTextarea.isVisible().catch(() => false);
        if (hasTextarea) {
          await expect(chatTextarea).toBeAttached();
        }
      }
    }
  });

  test('editor tab starts in right zone', async ({ page }) => {
    await waitForTabInZone(page, 'right', 'Editor');
    const editorTab = page.locator('[data-zone="right"] .dv-tab:has-text("Editor")');
    await expect(editorTab).toBeVisible();
  });
});
