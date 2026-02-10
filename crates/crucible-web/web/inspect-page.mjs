import { chromium } from '@playwright/test';

async function main() {
  const browser = await chromium.launch();
  const page = await browser.newPage();
  page.setViewportSize({ width: 1400, height: 900 });

  try {
    await page.goto('http://localhost:5173/flexlayout-test.html?layout=docked_panes', { waitUntil: 'networkidle' });
    await new Promise(resolve => setTimeout(resolve, 2000));
    
    // Get page structure
    const html = await page.content();
    console.log('=== PAGE STRUCTURE ===\n');
    console.log(html.substring(0, 3000));
    
    // Look for specific elements
    console.log('\n=== ELEMENT QUERIES ===\n');
    
    const tabs = await page.$$('[role="tab"]');
    console.log(`Tabs found: ${tabs.length}`);
    
    const buttons = await page.$$('button');
    console.log(`Buttons found: ${buttons.length}`);
    for (let i = 0; i < Math.min(buttons.length, 5); i++) {
      const btn = buttons[i];
      const text = await btn.textContent();
      const ariaLabel = await btn.getAttribute('aria-label');
      console.log(`  Button ${i}: text="${text}" aria-label="${ariaLabel}"`);
    }
    
    const borders = await page.$$('[data-border]');
    console.log(`Borders found: ${borders.length}`);
    
    const panes = await page.$$('[data-pane]');
    console.log(`Panes found: ${panes.length}`);

  } catch (error) {
    console.error('Error:', error);
  } finally {
    await browser.close();
  }
}

main();
