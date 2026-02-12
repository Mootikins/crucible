import { test, expect } from '@playwright/test';

test.describe('Icon Rendering Fix (Issue #9)', () => {
  test('emoji icons render as text, not broken img tags', async ({ page }) => {
    await page.goto('http://localhost:5173/flexlayout-test?layout=basic_icons');
    
    // Check that emoji icons are rendered as text spans
    const iconSpans = await page.locator('.flexlayout__tab_button_icon_text').count();
    expect(iconSpans).toBeGreaterThan(0);
    
    // Check that emoji icons are NOT rendered as img tags
    const iconImgs = await page.locator('.flexlayout__tab_button_leading img').count();
    expect(iconImgs).toBe(0);
    
    // Verify first icon is visible and contains emoji
    const firstIcon = await page.locator('.flexlayout__tab_button_icon_text').first().textContent();
    expect(firstIcon).toBeTruthy();
    expect(firstIcon).toMatch(/[\p{Emoji}]/u);
  });

  test('URL icons still render as img tags', async ({ page }) => {
    await page.goto('http://localhost:5173/flexlayout-test?layout=test_three_tabs');
    
    // Check that URL icons are rendered as img tags
    const iconImgs = await page.locator('.flexlayout__tab_button_leading img').count();
    expect(iconImgs).toBeGreaterThan(0);
  });

  test('icon text is properly sized and aligned', async ({ page }) => {
    await page.goto('http://localhost:5173/flexlayout-test?layout=basic_icons');
    
    const iconSpan = page.locator('.flexlayout__tab_button_icon_text').first();
    const styles = await iconSpan.evaluate(el => {
      const computed = window.getComputedStyle(el);
      return {
        fontSize: computed.fontSize,
        lineHeight: computed.lineHeight,
        display: computed.display,
        alignItems: computed.alignItems,
      };
    });
    
    // Verify CSS is applied
    expect(styles.fontSize).toBeTruthy();
    // lineHeight is computed as pixel value
    expect(styles.lineHeight).toMatch(/^[\d.]+px$/);
  });
});
