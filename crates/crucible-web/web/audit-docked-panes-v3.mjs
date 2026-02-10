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
    await sleep(2000);
    
    await screenshot(page, 'Phase1_Initial_State_Full_Page');
    
    // Inspect structure
    const tabButtons = await page.$$('.flexlayout__tab_button');
    console.log(`Found ${tabButtons.length} tab buttons`);
    
    const borders = await page.$$('.flexlayout__border');
    console.log(`Found ${borders.length} borders`);

    console.log('\n=== PHASE 2: TAB SELECTION ===\n');
    
    // Click each tab button (re-query each time to avoid stale elements)
    for (let i = 0; i < 3; i++) {
      const tabs = await page.$$('.flexlayout__tab_button');
      if (i >= tabs.length) break;
      
      const tab = tabs[i];
      const text = await tab.textContent();
      console.log(`  Clicking tab ${i}: "${text.trim()}"`);
      await tab.click();
      await sleep(300);
      await screenshot(page, `Phase2_Tab_Click_${i}_${text.trim().substring(0, 15).replace(/\s+/g, '_')}`);
    }

    console.log('\n=== PHASE 3: COLLAPSE/EXPAND ===\n');
    
    // Collapse top border
    let collapseBtn = await page.$('button:has-text("▲")');
    if (collapseBtn) {
      console.log(`  Collapsing top border...`);
      await collapseBtn.click();
      await sleep(400);
      await screenshot(page, 'Phase3_Collapse_Top_Border');
    }
    
    // Collapse left border
    collapseBtn = await page.$('button:has-text("◀")');
    if (collapseBtn) {
      console.log(`  Collapsing left border...`);
      await collapseBtn.click();
      await sleep(400);
      await screenshot(page, 'Phase3_Collapse_Left_Border');
    }
    
    // Collapse right border
    collapseBtn = await page.$('button:has-text("▶")');
    if (collapseBtn) {
      console.log(`  Collapsing right border...`);
      await collapseBtn.click();
      await sleep(400);
      await screenshot(page, 'Phase3_Collapse_Right_Border');
    }
    
    // Collapse bottom border
    collapseBtn = await page.$('button:has-text("▼")');
    if (collapseBtn) {
      console.log(`  Collapsing bottom border...`);
      await collapseBtn.click();
      await sleep(400);
      await screenshot(page, 'Phase3_Collapse_Bottom_Border');
    }

    console.log('\n=== PHASE 4: DRAG OPERATIONS ===\n');
    
    // Try dragging a tab
    const dragTabs = await page.$$('.flexlayout__tab_button');
    if (dragTabs.length > 0) {
      const sourceTab = dragTabs[0];
      const sourceBox = await sourceTab.boundingBox();
      
      console.log(`  Dragging tab from (${sourceBox.x}, ${sourceBox.y}) to center...`);
      
      await page.mouse.move(sourceBox.x + sourceBox.width / 2, sourceBox.y + sourceBox.height / 2);
      await sleep(100);
      await page.mouse.down();
      await sleep(100);
      await page.mouse.move(700, 450);
      await sleep(100);
      await page.mouse.up();
      await sleep(500);
      
      await screenshot(page, 'Phase4_Drag_Tab_To_Center');
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
    console.error(error.stack);
  } finally {
    await browser.close();
  }
}

main();
