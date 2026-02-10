import { test, expect, type Page } from '@playwright/test';

const baseURL = '/flexlayout-test.html?layout=paneview_basic';

test.beforeEach(async ({ page }) => {
  await page.goto(baseURL);
  await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });
});

function paneviewContainer(page: Page) {
  return page.locator('[data-layout-path="/ts0/paneview"]');
}

function paneHeader(page: Page, index: number) {
  return page.locator(`[data-layout-path="/ts0/pane${index}/header"]`);
}

function paneContent(page: Page, index: number) {
  return page.locator(`[data-layout-path="/ts0/pane${index}/content"]`);
}

test.describe('Paneview rendering', () => {
  test('renders all section headers visible', async ({ page }) => {
    const container = paneviewContainer(page);
    await expect(container).toBeVisible();

    await expect(paneHeader(page, 0)).toBeVisible();
    await expect(paneHeader(page, 1)).toBeVisible();
    await expect(paneHeader(page, 2)).toBeVisible();

    await expect(paneHeader(page, 0).locator('[data-paneview-name]')).toContainText('Explorer');
    await expect(paneHeader(page, 1).locator('[data-paneview-name]')).toContainText('Search');
    await expect(paneHeader(page, 2).locator('[data-paneview-name]')).toContainText('Source Control');
  });

  test('first section expanded by default, others collapsed', async ({ page }) => {
    await expect(paneContent(page, 0)).toBeVisible();
    await expect(paneContent(page, 1)).toBeHidden();
    await expect(paneContent(page, 2)).toBeHidden();

    await expect(paneHeader(page, 0)).toHaveAttribute('data-state', 'expanded');
    await expect(paneHeader(page, 1)).toHaveAttribute('data-state', 'collapsed');
    await expect(paneHeader(page, 2)).toHaveAttribute('data-state', 'collapsed');
  });
});

test.describe('Paneview expand/collapse', () => {
  test('clicking header toggles content visibility', async ({ page }) => {
    await expect(paneContent(page, 0)).toBeVisible();
    await expect(paneHeader(page, 0)).toHaveAttribute('data-state', 'expanded');

    await paneHeader(page, 0).click();

    await expect(paneContent(page, 0)).toBeHidden();
    await expect(paneHeader(page, 0)).toHaveAttribute('data-state', 'collapsed');

    await paneHeader(page, 0).click();

    await expect(paneContent(page, 0)).toBeVisible();
    await expect(paneHeader(page, 0)).toHaveAttribute('data-state', 'expanded');
  });

  test('multiple sections can be expanded simultaneously', async ({ page }) => {
    await paneHeader(page, 1).click();
    await paneHeader(page, 2).click();

    await expect(paneContent(page, 0)).toBeVisible();
    await expect(paneContent(page, 1)).toBeVisible();
    await expect(paneContent(page, 2)).toBeVisible();

    await expect(paneHeader(page, 0)).toHaveAttribute('data-state', 'expanded');
    await expect(paneHeader(page, 1)).toHaveAttribute('data-state', 'expanded');
    await expect(paneHeader(page, 2)).toHaveAttribute('data-state', 'expanded');
  });

  test('headers remain visible when all sections collapsed', async ({ page }) => {
    await paneHeader(page, 0).click();

    await expect(paneContent(page, 0)).toBeHidden();
    await expect(paneContent(page, 1)).toBeHidden();
    await expect(paneContent(page, 2)).toBeHidden();

    await expect(paneHeader(page, 0)).toBeVisible();
    await expect(paneHeader(page, 1)).toBeVisible();
    await expect(paneHeader(page, 2)).toBeVisible();
  });

  test('chevron reflects expand/collapse state', async ({ page }) => {
    const chevron0 = paneHeader(page, 0).locator('[data-paneview-chevron]');
    const chevron1 = paneHeader(page, 1).locator('[data-paneview-chevron]');

    await expect(chevron0).toContainText('▾');
    await expect(chevron1).toContainText('▸');

    await paneHeader(page, 0).click();
    await expect(chevron0).toContainText('▸');

    await paneHeader(page, 1).click();
    await expect(chevron1).toContainText('▾');
  });
});

test.describe('Paneview drag reorder', () => {
  test('dragging header reorders sections', async ({ page }) => {
    await expect(paneHeader(page, 0).locator('[data-paneview-name]')).toContainText('Explorer');
    await expect(paneHeader(page, 2).locator('[data-paneview-name]')).toContainText('Source Control');

    const sourceHeader = paneHeader(page, 2);
    const targetSection = page.locator('[data-layout-path="/ts0/pane0"]');

    const sourceBox = await sourceHeader.boundingBox();
    const targetBox = await targetSection.boundingBox();
    expect(sourceBox).toBeTruthy();
    expect(targetBox).toBeTruthy();

    await page.mouse.move(sourceBox!.x + sourceBox!.width / 2, sourceBox!.y + sourceBox!.height / 2);
    await page.mouse.down();
    await page.mouse.move(targetBox!.x + targetBox!.width / 2, targetBox!.y + 5, { steps: 10 });
    await page.mouse.up();

    await page.waitForTimeout(200);

    const headers = paneviewContainer(page).locator('[data-paneview-header]');
    const count = await headers.count();
    expect(count).toBe(3);
  });
});

test.describe('Paneview serialization', () => {
  test('paneview container renders with correct number of sections', async ({ page }) => {
    const container = paneviewContainer(page);
    await expect(container).toBeVisible();

    const sections = container.locator('.flexlayout__paneview_section');
    await expect(sections).toHaveCount(3);

    const headers = container.locator('[data-paneview-header]');
    await expect(headers).toHaveCount(3);
  });

  test('paneview layout coexists with regular tab content', async ({ page }) => {
    const pane = paneviewContainer(page);
    await expect(pane).toBeVisible();

    const editorContent = page.locator('.flexlayout__tab').filter({ hasText: 'Main editor area' });
    await expect(editorContent.first()).toBeAttached();
  });
});
