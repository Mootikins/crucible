import { test, expect, type Page } from '@playwright/test';
import { setupBasicMocks } from '../helpers/mock-api';
import { createStory } from './_helpers/story';

/**
 * Story: the real terminal (xterm.js over the PTY WebSocket).
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
    // The terminal is a PANE of a rail, not a bottom dock — that dock is gone,
    // and the pane ships collapsed to its tab strip. Open the rail, then the
    // pane, through the store: both the ribbon toggle and the pane marker
    // animate, and this story is about the PTY, not about hitting a chevron.
    await page.evaluate(() => {
      const store = (window as unknown as Record<string, any>).__windowStore;
      const actions = (window as unknown as Record<string, any>).__windowActions;
      const panes = (node: any): any[] =>
        !node || typeof node !== 'object'
          ? []
          : node.type === 'pane'
            ? [node]
            : [...panes(node.first), ...panes(node.second)];
      for (const pos of ['left', 'right'] as const) {
        const panel = store.edgePanels[pos];
        for (const pane of panes(panel.layout)) {
          const group = store.tabGroups[pane.tabGroupId];
          if (!group?.tabs.some((t: any) => t.contentType === 'terminal')) continue;
          if (panel.mode !== 'docked') actions.toggleEdgePanel(pos);
          actions.setPaneCollapsed(pane.id, false);
          const tab = group.tabs.find((t: any) => t.contentType === 'terminal');
          actions.setActiveTab(pane.tabGroupId, tab.id);
          return;
        }
      }
      throw new Error('no rail pane hosts a terminal tab');
    });

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
