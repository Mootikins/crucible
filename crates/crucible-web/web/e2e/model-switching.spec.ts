import { test, expect } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';

/**
 * E2E: Model Switching
 *
 * Verifies the model picker dropdown opens with available models
 * and that switching a model calls the correct API endpoint.
 */

test('model picker opens and shows available models', async ({ page }) => {
  await setupBasicMocks(page);
  await page.goto('/');

  // Click the session in the sidebar to open it in the chat tab
  const sessionItem = page.getByTestId('session-item-test-session-001');
  await expect(sessionItem).toBeVisible({ timeout: 5000 });
  await sessionItem.click();

  // Wait for model picker button to be visible and enabled
  const pickerButton = page.getByTestId('model-picker-button');
  await expect(pickerButton).toBeVisible({ timeout: 5000 });
  await expect(pickerButton).not.toBeDisabled({ timeout: 5000 });

  // Open the model picker dropdown
  await pickerButton.click();

  // Assert both models from MOCK_PROVIDERS are visible
  await expect(page.getByTestId('model-option-llama3.2')).toBeVisible();
  await expect(page.getByTestId('model-option-mistral')).toBeVisible();
});


test('switching model calls the API', async ({ page }) => {
  // Mock the model switch endpoint BEFORE setupBasicMocks
  await page.route('**/api/session/*/model', (route) => {
    if (route.request().method() === 'POST') {
      route.fulfill({ status: 200, body: '' });
    } else {
      route.continue();
    }
  });

  await setupBasicMocks(page);
  await page.goto('/');

  // Click the session in the sidebar to open it in the chat tab
  const sessionItem = page.getByTestId('session-item-test-session-001');
  await expect(sessionItem).toBeVisible({ timeout: 5000 });
  await sessionItem.click();

  // Wait for model picker button to be enabled
  const pickerButton = page.getByTestId('model-picker-button');
  await expect(pickerButton).not.toBeDisabled({ timeout: 5000 });

  // Open the picker
  await pickerButton.click();

  // Wait for model options to appear
  await expect(page.getByTestId('model-option-mistral')).toBeVisible();

  // Set up request interception before clicking
  const modelRequestPromise = page.waitForRequest(
    (req) => req.url().includes('/model') && req.method() === 'POST',
  );

  // Click mistral to switch
  await page.getByTestId('model-option-mistral').click();

  // Assert POST /api/session/{id}/model was called
  const request = await modelRequestPromise;
  expect(request.url()).toContain('/api/session/');
  expect(request.url()).toContain('/model');
  expect(request.method()).toBe('POST');
});
