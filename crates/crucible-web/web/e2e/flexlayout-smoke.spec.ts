import { test, expect } from "@playwright/test";

const HARNESS_URL = "/flexlayout-test.html";

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
    await expect(tabButtons).toHaveCount(3);

    await expect(tabButtons.nth(0)).toContainText("Tab 1");
    await expect(tabButtons.nth(1)).toContainText("Tab 1b");
    await expect(tabButtons.nth(2)).toContainText("Tab 2");

    const selectedTabs = page.locator(".flexlayout__tab_button--selected");
    await expect(selectedTabs).toHaveCount(2);

    const tabsets = page.locator(".flexlayout__tabset");
    await expect(tabsets).toHaveCount(2);

    await page.screenshot({
      path: "../../../.sisyphus/evidence/flexlayout-smoke-render.png",
    });
  });

  test("should allow tab selection by clicking", async ({ page }) => {
    const tabButtons = page.locator(".flexlayout__tab_button");
    await expect(tabButtons).toHaveCount(3);

    await expect(tabButtons.nth(0)).toHaveClass(/--selected/);
    await expect(tabButtons.nth(1)).not.toHaveClass(/--selected/);

    await page.evaluate(() => {
      const btns = document.querySelectorAll(".flexlayout__tab_button");
      (btns[1] as HTMLElement).click();
    });

    await expect(
      page.locator(".flexlayout__tab_button").nth(1),
    ).toHaveClass(/--selected/, { timeout: 3000 });

    await expect(
      page.locator(".flexlayout__tab_button").nth(0),
    ).toHaveClass(/--unselected/);

    const panelContent = page.locator('[data-testid="panel-Tab 1b"]');
    await expect(panelContent).toBeVisible();

    await page.screenshot({
      path: "../../../.sisyphus/evidence/flexlayout-smoke-tab-select.png",
    });
  });
});
