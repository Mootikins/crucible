import { test, expect } from '@playwright/test';

test.describe('Chat Interface', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    
    const projectButton = page.getByText('/home/moot/crucible').first();
    await expect(projectButton).toBeVisible();
    await projectButton.click();
    
    const sessionButton = page.locator('button:has(span[class*="rounded-full"])').first();
    await expect(sessionButton).toBeVisible();
    await sessionButton.click();
  });

  test('displays chat input area', async ({ page }) => {
    const chatInput = page.locator('textarea, input[type="text"]').first();
    await expect(chatInput).toBeVisible();
  });

  test('can type in chat input', async ({ page }) => {
    const chatInput = page.locator('textarea, input[type="text"]').first();
    
    await expect(chatInput).toBeVisible();
    await chatInput.fill('Hello, Crucible!');
    await expect(chatInput).toHaveValue('Hello, Crucible!');
  });

  test('displays send button', async ({ page }) => {
    const sendButton = page.locator('button:has-text("Send"), button[type="submit"]').first();
    await expect(sendButton).toBeVisible();
  });

  test('displays message list area', async ({ page }) => {
    const messageArea = page.locator('[class*="message"], [class*="chat"]');
    await expect(messageArea.first()).toBeVisible();
  });

  test('can send a message', async ({ page }) => {
    await page.route('**/api/sessions/*/messages', async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({ success: true }),
      });
    });

    const chatInput = page.locator('textarea, input[type="text"]').first();
    
    await expect(chatInput).toBeVisible();
    await chatInput.fill('Test message');
    await chatInput.press('Enter');
    
    await page.waitForTimeout(500);
  });

  test('displays microphone button for voice input', async ({ page }) => {
    const micButton = page.locator('button:has([class*="mic"]), button[aria-label*="microphone"]');
    await expect(micButton.first()).toBeVisible();
  });
});
