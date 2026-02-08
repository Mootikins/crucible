import { test, expect } from "@playwright/test";

const HARNESS_URL = "/flexlayout-test.html?layout=test_two_tabs";

test.describe("FlexLayout SolidJS — Smoke Tests", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(HARNESS_URL);
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });
  });

  test("should render 2-tabset layout with tab buttons visible", async ({
    page,
  }) => {
    const layoutRoot = page.locator('[data-layout-path="/"]');
    await expect(layoutRoot).toBeVisible();

    const tabButtons = page.locator(".flexlayout__tab_button");
    await expect(tabButtons).toHaveCount(2);

    await expect(tabButtons.nth(0)).toContainText("One");
    await expect(tabButtons.nth(1)).toContainText("Two");

    const selectedTabs = page.locator(".flexlayout__tab_button--selected");
    await expect(selectedTabs).toHaveCount(2);

    const tabsets = page.locator(".flexlayout__tabset");
    await expect(tabsets).toHaveCount(2);

    await page.screenshot({
      path: "../../../.sisyphus/evidence/flexlayout-smoke-render.png",
    });
  });

  test("should allow tab selection by clicking", async ({ page }) => {
    // Use test_three_tabs layout which has 3 tabs across 3 tabsets
    await page.goto("/flexlayout-test.html?layout=test_three_tabs");
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const tabButtons = page.locator(".flexlayout__tab_button");
    await expect(tabButtons).toHaveCount(3);

    // First tab in first tabset should be selected
    await expect(tabButtons.nth(0)).toHaveClass(/--selected/);

    // Click the second tab button using Playwright locators
    await tabButtons.nth(1).click();

    await expect(tabButtons.nth(1)).toHaveClass(/--selected/, {
      timeout: 3000,
    });

    // The panel content should be visible with data-testid
    const panelContent = page.locator('[data-testid="panel-Two"]');
    await expect(panelContent).toBeVisible();

    await page.screenshot({
      path: "../../../.sisyphus/evidence/flexlayout-smoke-tab-select.png",
    });
  });
});
