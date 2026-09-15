import { test, expect } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';
import { MOCK_SESSION, MOCK_SESSION_2 } from './helpers/fixtures';
import { openSessionsList } from './helpers/nav';

/**
 * E2E: where New Session lives in the sessions rail.
 *
 * It used to be one button over a flat list, and this spec asserted it sat
 * above the rows. The rail is now two tiers — project over session — and New
 * Session belongs to the PROJECT row, because a session belongs to exactly
 * one project and a panel-wide button could not say which one it meant.
 *
 * The claim survives in the form that still means something: the affordance
 * for a project sits on that project's own header, above the sessions it
 * would join.
 */
test.describe('New Session lives on the project row', () => {
  test('the project header carries New Session, above its sessions', async ({ page }) => {
    // Five newer sessions fill the Inbox, so MOCK_SESSION is a row under
    // its project header rather than an inbox row above every header.
    const fillers = Array.from({ length: 5 }, (_, i) => ({
      ...MOCK_SESSION,
      session_id: `filler-${i}`,
      title: `Filler ${i}`,
      started_at: `2026-06-0${i + 1}T00:00:00Z`,
      last_activity: `2026-06-0${i + 1}T00:00:00Z`,
    }));
    await setupBasicMocks(page, { sessions: [...fillers, MOCK_SESSION, MOCK_SESSION_2] });

    await page.goto('/');
    await openSessionsList(page);

    const sessionList = page.getByTestId('session-list');
    await expect(sessionList).toBeVisible({ timeout: 10000 });

    // MOCK_SESSION's workspace is MOCK_PROJECT's path, so both rows group here.
    const groupRow = page.getByTestId('session-group-/home/user/project');
    await expect(groupRow).toBeVisible({ timeout: 5000 });

    const newSessionBtn = page.getByTestId('session-group-new-/home/user/project');
    await expect(newSessionBtn).toBeAttached({ timeout: 5000 });
    // Transparent until hover — Playwright counts that as visible, so hover
    // before measuring or the box would be of a control no human can reach.
    await groupRow.hover();

    const firstSessionItem = page.getByTestId('session-item-test-session-001');
    await expect(firstSessionItem).toBeVisible({ timeout: 5000 });

    const buttonBox = await newSessionBtn.boundingBox();
    const firstItemBox = await firstSessionItem.boundingBox();
    expect(buttonBox).toBeTruthy();
    expect(firstItemBox).toBeTruthy();

    if (buttonBox && firstItemBox) {
      expect(buttonBox.y).toBeLessThan(firstItemBox.y);
    }
  });

  test('the panel carries no project-less New Session button', async ({ page }) => {
    await setupBasicMocks(page, { sessions: [MOCK_SESSION, MOCK_SESSION_2] });

    await page.goto('/');
    await openSessionsList(page);

    // The one that could not name its project is gone. The project-agnostic
    // entry point is Ctrl+Shift+N (and its palette row) — the ribbon's plus
    // was a third doorway to the same event and has been retired.
    await expect(page.getByTestId('new-session-button')).toHaveCount(0);
    await page.keyboard.press('Control+Shift+N');
    await expect(page.getByTestId('composer-input')).toBeVisible({ timeout: 15000 });
  });
});
