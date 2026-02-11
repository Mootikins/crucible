import { test, expect } from "@playwright/test";

const SPIKE_URL = "/reparenting-spike.html";

test.describe("SolidJS Reparenting Spike", () => {
  test("counter state survives DOM reparenting via appendChild", async ({ page }) => {
    await page.goto(SPIKE_URL);

    const counter = page.getByTestId("counter-value");
    const cleanupStatus = page.getByTestId("cleanup-status");
    const moveButton = page.getByTestId("move-button");
    const tabsetA = page.getByTestId("tabset-a");
    const tabsetB = page.getByTestId("tabset-b");

    await expect(counter).toBeVisible();
    await expect(tabsetA.locator(".tab-content-container")).toHaveCount(1);
    await expect(tabsetB.locator(".tab-content-container")).toHaveCount(0);

    await page.waitForTimeout(500);

    const counterTextBefore = await counter.textContent();
    const valueBefore = parseInt(counterTextBefore!.replace("Count: ", ""), 10);
    expect(valueBefore).toBeGreaterThan(0);

    await moveButton.click();

    await expect(tabsetA.locator(".tab-content-container")).toHaveCount(0);
    await expect(tabsetB.locator(".tab-content-container")).toHaveCount(1);

    await page.waitForTimeout(300);

    const counterTextAfter = await counter.textContent();
    const valueAfter = parseInt(counterTextAfter!.replace("Count: ", ""), 10);

    expect(valueAfter).toBeGreaterThan(valueBefore);
    await expect(cleanupStatus).toHaveAttribute("data-cleanup-called", "false");

    await moveButton.click();

    await expect(tabsetA.locator(".tab-content-container")).toHaveCount(1);
    await expect(tabsetB.locator(".tab-content-container")).toHaveCount(0);

    await page.waitForTimeout(300);

    const counterTextFinal = await counter.textContent();
    const valueFinal = parseInt(counterTextFinal!.replace("Count: ", ""), 10);

    expect(valueFinal).toBeGreaterThan(valueAfter);
    await expect(cleanupStatus).toHaveAttribute("data-cleanup-called", "false");
  });
});
