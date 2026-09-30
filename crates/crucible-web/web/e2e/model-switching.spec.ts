import { test, expect } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';
import { openSessionsList } from './helpers/nav';

/**
 * E2E: Model Switching
 *
 * Verifies the model picker dropdown opens with available models
 * and that switching a model calls the correct API endpoint.
 */

test('switching model calls the API', async ({ page }) => {
  // Mock the knob endpoint BEFORE setupBasicMocks. Every session knob —
  // model included — rides this one method now (step 13 of the
  // simplification plan), reached through `POST /api/rpc/session.knob.set`
  // since step 19 item 9 moved the browser onto `rpc(method, params)`.
  await page.route('**/api/rpc/session.knob.set', (route) => route.fulfill({ status: 200, body: '' }));

  await setupBasicMocks(page);
  await page.goto('/');
  await openSessionsList(page);

  // Click the session in the sidebar to open it in the chat tab
  const sessionItem = page.getByTestId('session-item-test-session-001');
  await expect(sessionItem).toBeVisible({ timeout: 5000 });
  await sessionItem.click();

  // Wait for the button to exist before asking whether it is enabled.
  // `not.toBeDisabled()` passes vacuously on an element that is not there yet,
  // so on its own it is not a readiness gate — it lets the test run ahead of
  // the session load and spend its budget before the models are ever fetched.
  const pickerButton = page.getByTestId('model-picker-button');
  await expect(pickerButton).toBeVisible({ timeout: 5000 });
  await expect(pickerButton).not.toBeDisabled({ timeout: 5000 });

  // Open the picker
  await pickerButton.click();

  // Wait for model options to appear
  await expect(page.getByTestId('model-option-llama3.2')).toBeVisible();
  await expect(page.getByTestId('model-option-mistral')).toBeVisible();

  // Set up request interception before clicking
  const modelRequestPromise = page.waitForRequest(
    (req) => req.url().includes('/api/rpc/session.knob.set') && req.method() === 'POST',
  );

  // Click mistral to switch
  await page.getByTestId('model-option-mistral').click();

  // Assert POST /api/rpc/session.knob.set was called with the model knob
  const request = await modelRequestPromise;
  expect(request.postDataJSON()).toEqual({
    session_id: 'test-session-001',
    knob: 'model',
    value: 'mistral',
  });
});
