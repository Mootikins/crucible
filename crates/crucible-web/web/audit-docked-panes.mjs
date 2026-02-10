import { chromium } from '@playwright/test';
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const EVIDENCE_DIR = '/home/moot/crucible/.sisyphus/evidence';
let screenshotCounter = 1;

async function screenshot(page, description) {
  const filename = `audit-${String(screenshotCounter).padStart(3, '0')}-${description.replace(/\s+/g, '_').toLowerCase()}.png`;
  const filepath = path.join(EVIDENCE_DIR, filename);
  await page.screenshot({ path: filepath, fullPage: true });
  console.log(`✓ Screenshot ${screenshotCounter}: ${description} → ${filename}`);
  screenshotCounter++;
  return filepath;
}

async function sleep(ms) {
  return new Promise(resolve => setTimeout(resolve, ms));
}

async function main() {
  const browser = await chromium.launch();
  const page = await browser.newPage();
  page.setViewportSize({ width: 1400, height: 900 });

  try {
    console.log('\n=== PHASE 1: INITIAL STATE ===\n');
    
    await page.goto('http://localhost:5173/flexlayout-test.html?layout=docked_panes', { waitUntil: 'networkidle' });
    await sleep(1000);
    
    await screenshot(page, 'Phase1_Initial_State_Full_Page');

    console.log('\n=== PHASE 2: TAB SELECTION ===\n');
    
    // Get all tabs and click them
    const tabs = await page.$$('[role="tab"]');
    console.log(`Found ${tabs.length} tabs`);
    
    for (let i = 0; i < Math.min(tabs.length, 6); i++) {
      const tab = tabs[i];
      const tabText = await tab.textContent();
      await tab.click();
      await sleep(300);
      await screenshot(page, `Phase2_Tab_Click_${i}_${tabText.trim().replace(/\s+/g, '_').substring(0, 20)}`);
    }

    console.log('\n=== PHASE 3: COLLAPSE/EXPAND CYCLES ===\n');
    
    // Find collapse buttons - look for buttons with aria-label or data attributes
    const allButtons = await page.$$('button');
    console.log(`Found ${allButtons.length} buttons total`);
    
    let collapseCount = 0;
    for (const btn of allButtons) {
      const ariaLabel = await btn.getAttribute('aria-label');
      const title = await btn.getAttribute('title');
      if ((ariaLabel && ariaLabel.toLowerCase().includes('collapse')) || 
          (title && title.toLowerCase().includes('collapse'))) {
        await btn.click();
        await sleep(400);
        await screenshot(page, `Phase3_Collapse_${collapseCount}_${ariaLabel || title}`);
        collapseCount++;
        if (collapseCount >= 4) break;
      }
    }

    console.log('\n=== PHASE 4: DRAG OPERATIONS ===\n');
    
    // Drag a tab from bottom to center
    const bottomTabs = await page.$$('[data-border="bottom"] [role="tab"]');
    if (bottomTabs.length > 0) {
      const tab = bottomTabs[0];
      const tabText = await tab.textContent();
      const tabBox = await tab.boundingBox();
      
      // Drag to center
      await page.mouse.move(tabBox.x + tabBox.width / 2, tabBox.y + tabBox.height / 2);
      await page.mouse.down();
      await sleep(100);
      await page.mouse.move(700, 450);
      await sleep(100);
      await page.mouse.up();
      await sleep(500);
      
      await screenshot(page, `Phase4_Drag_Bottom_Tab_To_Center`);
    }

    console.log('\n=== PHASE 5: RESIZE ===\n');
    
    // Resize window
    await page.setViewportSize({ width: 800, height: 600 });
    await sleep(500);
    await screenshot(page, 'Phase5_Resized_Small_800x600');
    
    await page.setViewportSize({ width: 1920, height: 1080 });
    await sleep(500);
    await screenshot(page, 'Phase5_Resized_Large_1920x1080');
    
    // Back to normal
    await page.setViewportSize({ width: 1400, height: 900 });
    await sleep(500);
    await screenshot(page, 'Phase5_Resized_Back_Normal');

    console.log('\n=== AUDIT COMPLETE ===\n');
    console.log(`Total screenshots taken: ${screenshotCounter - 1}`);
    console.log(`Evidence saved to: ${EVIDENCE_DIR}`);

  } catch (error) {
    console.error('Error during audit:', error);
  } finally {
    await browser.close();
  }
}

main();
