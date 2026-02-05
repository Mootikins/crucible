import { test, expect } from '@playwright/test';

test.describe('Project and Session Management', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
  });

  test('displays project selection interface', async ({ page }) => {
    await expect(page.locator('text=Projects')).toBeVisible();
    await expect(page.locator('text=+ Add Project')).toBeVisible();
  });

  test('can add a new project', async ({ page }) => {
    await page.click('text=+ Add Project');
    
    const input = page.locator('input[placeholder="/path/to/project"]');
    await expect(input).toBeVisible();
    
    await input.fill('/home/moot/crucible');
    await page.click('text=Add');
    
    await expect(page.locator('text=/home/moot/crucible')).toBeVisible();
  });

  test('displays sessions section when project selected', async ({ page }) => {
    const projectButton = page.locator('text=/home/moot/crucible').first();
    if (await projectButton.isVisible()) {
      await projectButton.click();
      await expect(page.locator('text=Sessions')).toBeVisible();
      await expect(page.locator('text=+ New Session')).toBeVisible();
    }
  });

  test('can create a new session', async ({ page }) => {
    const projectButton = page.locator('text=/home/moot/crucible').first();
    if (await projectButton.isVisible()) {
      await projectButton.click();
      
      await page.click('text=+ New Session');
      
      await page.waitForTimeout(1000);
      
      const sessionItems = page.locator('[class*="bg-blue-900"]');
      await expect(sessionItems.first()).toBeVisible();
    }
  });

  test('displays session state indicator', async ({ page }) => {
    const projectButton = page.locator('text=/home/moot/crucible').first();
    if (await projectButton.isVisible()) {
      await projectButton.click();
      
      const stateIndicator = page.locator('span[class*="rounded-full"]').first();
      if (await stateIndicator.isVisible()) {
        await expect(stateIndicator).toHaveAttribute('title', /active|paused|ended/);
      }
    }
  });

  test('can switch between sessions', async ({ page }) => {
    const projectButton = page.locator('text=/home/moot/crucible').first();
    if (await projectButton.isVisible()) {
      await projectButton.click();
      
      const sessions = page.locator('button:has(span[class*="rounded-full"])');
      const sessionCount = await sessions.count();
      
      if (sessionCount >= 2) {
        await sessions.nth(0).click();
        await expect(sessions.nth(0)).toHaveClass(/bg-blue-900/);
        
        await sessions.nth(1).click();
        await expect(sessions.nth(1)).toHaveClass(/bg-blue-900/);
      }
    }
  });

  test('displays session controls when session active', async ({ page }) => {
    const projectButton = page.locator('text=/home/moot/crucible').first();
    if (await projectButton.isVisible()) {
      await projectButton.click();
      
      const sessionButton = page.locator('button:has(span[class*="rounded-full"])').first();
      if (await sessionButton.isVisible()) {
        await sessionButton.click();
        
        const controls = page.locator('text=/Pause|Resume|End/');
        await expect(controls.first()).toBeVisible();
      }
    }
  });
});
