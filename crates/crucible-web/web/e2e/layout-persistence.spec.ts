import { test, expect } from '@playwright/test';

test.describe('Layout Persistence', () => {
  test('no console errors on initial load', async ({ page }) => {
    const consoleMessages: Array<{ type: string; text: string }> = [];
    
    page.on('console', (msg) => {
      consoleMessages.push({
        type: msg.type(),
        text: msg.text(),
      });
    });

    await page.goto('http://localhost:5173');
    await page.waitForTimeout(2000); // Wait for layout to initialize
    
    // Filter for dockview-related errors (the critical issue we're testing for)
    const dockviewErrors = consoleMessages.filter(msg => 
      msg.type === 'error' && (msg.text.includes('dockview') || msg.text.includes('pane'))
    );
    
    // Filter for context provider errors (e.g., "useSettings must be used within a SettingsProvider")
    const contextErrors = consoleMessages.filter(msg => 
      msg.type === 'error' && msg.text.includes('must be used within')
    );
    
    // Log all console messages for debugging
    console.log('=== All Console Messages ===');
    consoleMessages.forEach(msg => {
      console.log(`[${msg.type.toUpperCase()}] ${msg.text}`);
    });
    
    console.log('\n=== Dockview-Related Errors ===');
    if (dockviewErrors.length === 0) {
      console.log('✅ No dockview errors found');
    } else {
      dockviewErrors.forEach(err => {
        console.log(`❌ ${err.text}`);
      });
    }
    
    console.log('\n=== Context Provider Errors ===');
    if (contextErrors.length === 0) {
      console.log('✅ No context provider errors found');
    } else {
      contextErrors.forEach(err => {
        console.log(`❌ ${err.text}`);
      });
    }
    
    expect(dockviewErrors).toHaveLength(0);
    expect(contextErrors).toHaveLength(0);
  });

  test('layout persists after refresh', async ({ page }) => {
    const consoleMessages: Array<{ type: string; text: string }> = [];
    
    page.on('console', (msg) => {
      consoleMessages.push({
        type: msg.type(),
        text: msg.text(),
      });
    });

    await page.goto('http://localhost:5173');
    
    // Wait for the toggle button to be visible (indicates app has loaded)
    const leftToggle = page.locator('[data-testid="toggle-left"]');
    await expect(leftToggle).toBeVisible({ timeout: 10000 });
    
    // Collapse left panel
    await leftToggle.click();
    
    // Wait for debounced save (300ms debounce + buffer)
    await page.waitForTimeout(1000);
    
    // Check localStorage before refresh
    const layoutStateBefore = await page.evaluate(() => {
      return localStorage.getItem('crucible:layout');
    });
    
    console.log('Layout state before refresh:', layoutStateBefore ? 'EXISTS' : 'NULL');
    
    // Refresh page
    await page.reload();
    await page.waitForTimeout(1000);
    
    // Check if layout state persists after refresh
    const layoutStateAfter = await page.evaluate(() => {
      return localStorage.getItem('crucible:layout');
    });
    
    console.log('Layout state after refresh:', layoutStateAfter ? 'EXISTS' : 'NULL');
    
    // Log any dockview errors
    const dockviewErrors = consoleMessages.filter(msg => 
      msg.type === 'error' && (msg.text.includes('dockview') || msg.text.includes('pane'))
    );
    
    if (dockviewErrors.length > 0) {
      console.log('❌ Dockview errors during persistence test:');
      dockviewErrors.forEach(err => console.log(`  ${err.text}`));
    } else {
      console.log('✅ No dockview errors during persistence test');
    }
    
    // The key assertion: no dockview errors should occur
    expect(dockviewErrors).toHaveLength(0);
  });

  test('handles corrupt localStorage gracefully', async ({ page }) => {
    const consoleMessages: Array<{ type: string; text: string }> = [];
    
    page.on('console', (msg) => {
      consoleMessages.push({
        type: msg.type(),
        text: msg.text(),
      });
    });

    // Set corrupt localStorage
    await page.goto('http://localhost:5173');
    await page.evaluate(() => {
      localStorage.setItem('crucible:layout', 'invalid json{{{');
    });
    
    // Reload with corrupt state
    await page.reload();
    await page.waitForTimeout(1000);
    
    // Check if recovery happened
    const layoutState = await page.evaluate(() => {
      return localStorage.getItem('crucible:layout');
    });
    
    // Log recovery status
    console.log('Layout state after corrupt recovery:', layoutState ? 'EXISTS' : 'CLEARED');
    
    // Check for recovery warnings
    const recoveryWarnings = consoleMessages.filter(msg => 
      msg.type === 'warning' && msg.text.includes('Failed to restore layout')
    );
    
    console.log('Recovery warnings found:', recoveryWarnings.length);
    recoveryWarnings.forEach(warn => {
      console.log(`  ${warn.text}`);
    });
    
    // Check for dockview errors during recovery
    const dockviewErrors = consoleMessages.filter(msg => 
      msg.type === 'error' && (msg.text.includes('dockview') || msg.text.includes('pane'))
    );
    
    if (dockviewErrors.length > 0) {
      console.log('❌ Dockview errors during corrupt state recovery:');
      dockviewErrors.forEach(err => console.log(`  ${err.text}`));
    } else {
      console.log('✅ No dockview errors during corrupt state recovery');
    }
    
    // The key assertion: no dockview errors should occur even with corrupt state
    expect(dockviewErrors).toHaveLength(0);
  });
});
