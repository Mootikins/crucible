import { test, expect } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';
import { appReady, openNewSessionTab, openSession } from './helpers/nav';

/**
 * E2E: the new-session isolation control.
 *
 * The composer offers whatever a plugin published to
 * `GET /api/plugins/publications` and hands
 * the pick straight to `POST /api/session`. This spec watches the actual
 * request body, because the whole feature is "the value reaches the server
 * unrewritten" — and because `false` and *absent* are different instructions
 * that a truthiness bug would silently collapse into one.
 */

/** Type a first message and submit, returning the create request's body. */
async function submitAndCaptureCreate(
  page: import('@playwright/test').Page,
  text: string,
): Promise<Record<string, unknown>> {
  const request = page.waitForRequest(
    (req) => req.url().endsWith('/api/session') && req.method() === 'POST',
  );
  await page.getByTestId('composer-input').fill(text);
  await page.getByTestId('composer-send').click();
  return JSON.parse((await request).postData() ?? '{}');
}

test('the isolation toggle and profile pick ride through session create', async ({ page }) => {
  await setupBasicMocks(page);
  await page.goto('/');
  await openNewSessionTab(page);

  const chip = page.getByTestId('composer-isolation');
  await expect(chip).toBeVisible();
  await chip.click();

  // The toggle starts untouched, and the popout lists the server's profiles.
  const toggle = page.getByTestId('isolation-toggle');
  await expect(toggle).toHaveAttribute('aria-pressed', 'false');
  await expect(page.getByTestId('composer-isolation-popout')).toContainText('throwaway');

  await page.getByRole('option', { name: 'throwaway' }).click();
  await expect(chip).toContainText('throwaway');

  expect(await submitAndCaptureCreate(page, 'sandbox me')).toMatchObject({
    isolation: 'throwaway',
  });
});

test('an untouched composer sends no isolation field at all', async ({ page }) => {
  await setupBasicMocks(page);
  await page.goto('/');
  await openNewSessionTab(page);
  await expect(page.getByTestId('composer-isolation')).toBeVisible();

  // Absent means "resolve normally" — the server's own setting still applies.
  const body = await submitAndCaptureCreate(page, 'just start');
  expect(body).not.toHaveProperty('isolation');
});

test('a server offering no isolation shows no isolation control', async ({ page }) => {
  await setupBasicMocks(page, {
    publications: { publications: { isolation: { oci: { available: false, profiles: [] } } } },
  });
  await page.goto('/');
  await openNewSessionTab(page);

  await expect(page.getByTestId('composer-kiln')).toBeVisible();
  await expect(page.getByTestId('composer-isolation')).toHaveCount(0);
});

// The regression the publication channel exists for: a bare-image config
// publishes availability with no profile names, and the control must still be
// there — otherwise there is no way to opt a session OUT of isolation.
test('a server offering isolation with no named profiles still shows the control', async ({
  page,
}) => {
  await setupBasicMocks(page, {
    publications: { publications: { isolation: { oci: { available: true, profiles: [] } } } },
  });
  await page.goto('/');
  await openNewSessionTab(page);

  await expect(page.getByTestId('composer-isolation')).toBeVisible();
});

test("plugin status slots render as chips the frontend doesn't interpret", async ({ page }) => {
  await setupBasicMocks(page, {
    sessionStatus: {
      status: [
        { key: 'zarquon', plugin: 'zarquon', text: 'flux capacitor charged', level: 'info' },
      ],
    },
  });
  await page.goto('/');
  await appReady(page);
  await openSession(page, 'test-session-001');

  // A key this frontend has never heard of still reaches the strip.
  await expect(page.getByTestId('session-status-zarquon')).toContainText(
    'flux capacitor charged',
    { timeout: 15000 },
  );
});
