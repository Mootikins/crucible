import { test, expect } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';
import { appReady, openNewSessionTab, openSession } from './helpers/nav';

/**
 * E2E: the new-session target chips, on both axes.
 *
 * The composer offers whatever providers published to
 * `GET /api/plugins/publications` and enumerated through
 * `POST /api/plugins/command`, then hands the pick straight to
 * `POST /api/session`. This spec watches the actual request body, because the
 * whole feature is "the pick reaches the server unrewritten" — and because
 * `false` and *absent* are different instructions that a truthiness bug would
 * silently collapse into one.
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

test('a runtime target rides through session create, addressed to its provider', async ({
  page,
}) => {
  await setupBasicMocks(page);
  await page.goto('/');
  await openNewSessionTab(page);

  const chip = page.getByTestId('composer-target');
  await expect(chip).toBeVisible();
  await chip.click();

  const popout = page.getByTestId('composer-target-popout');
  // This machine is built in; the profiles came from the provider.
  await expect(popout).toContainText('This PC');
  await expect(popout).toContainText('throwaway');

  await page.getByRole('option', { name: 'throwaway' }).click();
  await expect(chip).toContainText('throwaway');

  // Addressed, not a bare name: more than one plugin answers on this channel,
  // and a name meant for one used to be a hard error inside another.
  expect(await submitAndCaptureCreate(page, 'sandbox me')).toMatchObject({
    isolation: { plugin: 'oci', target: 'throwaway' },
  });
});

test('a workspace target rides through as a provider-addressed spec', async ({ page }) => {
  await setupBasicMocks(page);
  await page.goto('/');
  await openNewSessionTab(page);

  const chip = page.getByTestId('composer-workspace-target');
  await expect(chip).toBeVisible();
  await chip.click();
  await page.getByRole('option', { name: 'feat/x' }).click();

  // The daemon splits on the first colon to find who resolves it, and does so
  // BEFORE creating the session — so the session is born in that checkout.
  expect(await submitAndCaptureCreate(page, 'on a worktree')).toMatchObject({
    workspace_target: 'worktree:feat/x',
  });
});

test('both axes ride through together', async ({ page }) => {
  await setupBasicMocks(page);
  await page.goto('/');
  await openNewSessionTab(page);

  await page.getByTestId('composer-workspace-target').click();
  await page.getByRole('option', { name: 'feat/x' }).click();
  await page.getByTestId('composer-target').click();
  await page.getByRole('option', { name: 'rust rust:1-bookworm' }).click();

  // The combination the oci plugin already assumed worked and nothing could
  // express: a container running against a worktree.
  expect(await submitAndCaptureCreate(page, 'both')).toMatchObject({
    workspace_target: 'worktree:feat/x',
    isolation: { plugin: 'oci', target: 'rust' },
  });
});

test('an untouched composer sends neither axis', async ({ page }) => {
  await setupBasicMocks(page);
  await page.goto('/');
  await openNewSessionTab(page);
  await expect(page.getByTestId('composer-target')).toBeVisible();

  // Absent means "resolve normally" — the project's own setting still applies.
  const body = await submitAndCaptureCreate(page, 'just start');
  expect(body).not.toHaveProperty('isolation');
  expect(body).not.toHaveProperty('workspace_target');
});

test('choosing this machine explicitly opts the session out of isolation', async ({ page }) => {
  await setupBasicMocks(page);
  await page.goto('/');
  await openNewSessionTab(page);

  await page.getByTestId('composer-target').click();
  await page.getByRole('option', { name: 'This PC' }).click();

  // Distinct from sending nothing: this overrides a project that asks for a
  // container, which is the only way to say "not isolated" out loud.
  expect(await submitAndCaptureCreate(page, 'on the host')).toMatchObject({ isolation: false });
});

test('a box with no providers still offers this machine, and no workspace chip', async ({
  page,
}) => {
  await setupBasicMocks(page, { publications: { publications: {} } });
  await page.goto('/');
  await openNewSessionTab(page);

  await expect(page.getByTestId('composer-kiln')).toBeVisible();
  // Running here cannot depend on a plugin being installed.
  await page.getByTestId('composer-target').click();
  await expect(page.getByTestId('composer-target-popout')).toContainText('This PC');
  // But a chip that could only ever be empty is worse than none.
  await expect(page.getByTestId('composer-workspace-target')).toHaveCount(0);
});

// A second provider on an axis earns the drill-down; one provider flattens,
// because a submenu holding the whole menu is an extra click.
test('a second provider on an axis turns the menu into a drill-down', async ({ page }) => {
  await setupBasicMocks(page, {
    publications: {
      publications: {
        targets: {
          oci: { axis: 'runtime', label: 'Container', targets_command: 'oci.targets' },
          ssh: { axis: 'runtime', label: 'Remote Machines', targets_command: 'ssh.targets' },
        },
      },
    },
    pluginTargets: {
      'oci.targets': { targets: [{ value: 'rust', label: 'rust' }] },
      'ssh.targets': { targets: [{ value: 'build-box', label: 'build-box' }] },
    },
  });
  await page.goto('/');
  await openNewSessionTab(page);

  await page.getByTestId('composer-target').click();
  const popout = page.getByTestId('composer-target-popout');
  await expect(popout).toContainText('Remote Machines');
  // The hosts stay behind the door until it is opened.
  await expect(popout).not.toContainText('build-box');

  // Hover, not click: a submenu that needs a click to open is a submenu the
  // pointer has already told you it wants.
  await page.getByRole('option', { name: 'Remote Machines' }).hover();
  await expect(page.getByTestId('composer-target-flyout')).toContainText('build-box');

  await page.getByRole('option', { name: 'build-box' }).click();
  expect(await submitAndCaptureCreate(page, 'over ssh')).toMatchObject({
    isolation: { plugin: 'ssh', target: 'build-box' },
  });
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
