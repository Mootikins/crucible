import { test, expect, type Page } from '@playwright/test';
import { setupBasicMocks } from '../helpers/mock-api';
import { createStory } from './_helpers/story';

/**
 * Story: real terminal in the bottom panel (xterm.js over the PTY WebSocket).
 *
 * The PTY endpoint is mocked with routeWebSocket (no real shell spawns in
 * CI): the mock "server" greets with a prompt, echoes typed input back, and
 * can drop the connection to exercise the reconnect affordance.
 *
 * Validated behaviors:
 *  1. The Terminal tab hosts a real xterm instance (not the old line-based
 *     command runner) and renders the PTY greeting.
 *  2. Keystrokes round-trip: typed input reaches the socket as {t:'i'}
 *     frames and the echoed bytes render in the terminal.
 *  3. A dropped connection surfaces the reconnect button; reconnecting
 *     opens a fresh socket and paints a fresh prompt.
 */

async function mockPty(page: Page, state: { sockets: import('@playwright/test').WebSocketRoute[] }) {
  await page.routeWebSocket('**/api/terminal/ws', (ws) => {
    state.sockets.push(ws);
    ws.send('mock-pty$ ');
    ws.onMessage((message) => {
      try {
        const msg = JSON.parse(String(message));
        if (msg.t === 'i') ws.send(msg.d);
      } catch {
        // resize frames etc. — ignore
      }
    });
  });
}

test.describe('Terminal panel (xterm over PTY WebSocket)', () => {
  test('renders a PTY prompt, echoes input, and reconnects after a drop', async ({ page }, testInfo) => {
    const story = createStory(testInfo);
    const state = { sockets: [] as import('@playwright/test').WebSocketRoute[] };
    await setupBasicMocks(page, { sseEvents: [] });
    await mockPty(page, state);

    await page.goto('/');
    // Expand the bottom panel via its ribbon toggle and pick the Terminal tab.
    await page.getByTestId('ribbon-toggle-bottom').click();
    await page.getByTestId('edge-tab-bottom-terminal-tab-1').click();

    // 1. A real xterm mounted and shows the mock PTY's greeting.
    const panel = page.getByTestId('terminal-panel');
    await expect(panel.locator('.xterm')).toBeVisible({ timeout: 5000 });
    await expect(panel).toContainText('mock-pty$', { timeout: 5000 });
    await story.step(page, 'terminal open with prompt');

    // 2. Typing round-trips through the socket (mock echoes it back).
    await panel.locator('.xterm').click();
    await page.keyboard.type('hello');
    await expect(panel).toContainText('hello', { timeout: 5000 });
    await story.step(page, 'input echoed');

    // 3. Server-side drop → reconnect affordance → fresh session.
    state.sockets[0].close();
    const reconnect = page.getByTestId('terminal-reconnect');
    await expect(reconnect).toBeVisible({ timeout: 5000 });
    await story.step(page, 'connection dropped');

    await reconnect.click();
    await expect(reconnect).toBeHidden({ timeout: 5000 });
    await expect(panel).toContainText('mock-pty$', { timeout: 5000 });
    expect(state.sockets.length).toBe(2);
    await story.step(page, 'reconnected');
  });
});
