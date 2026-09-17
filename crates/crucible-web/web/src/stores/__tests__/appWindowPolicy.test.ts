import { describe, it, expect, vi, afterEach } from 'vitest';

const terminal = vi.hoisted(() => ({ allowed: true }));
vi.mock('@/lib/terminal-availability', () => ({ terminalAllowed: () => terminal.allowed }));

import { appWindowPolicy } from '@/stores/windowStore';
import { DEFAULT_SHORTCUTS } from '@/lib/keyboard-shortcuts';
import { LAYOUT_ACTIONS } from '@/windowing/shortcuts';
import { getBus } from '@/lib/bus';

const keydown = () => new KeyboardEvent('keydown');

afterEach(() => vi.restoreAllMocks());

describe('appWindowPolicy.onShortcut', () => {
  it('consumes every app chord in its table, so the browser default stays off', () => {
    const appActions = DEFAULT_SHORTCUTS.map((s) => s.action).filter((a) => !LAYOUT_ACTIONS.has(a));
    for (const action of appActions) {
      expect(appWindowPolicy.onShortcut(action, keydown()), action).toBe(true);
    }
  });

  it('asks for a clear through the bus', () => {
    let asked = 0;
    const off = getBus().on('clearChat', () => { asked += 1; });
    appWindowPolicy.onShortcut('clearChat', keydown());
    off();
    expect(asked).toBe(1);
  });
});

describe('appWindowPolicy.unavailableReason', () => {
  const term = { id: 't', title: 'Terminal', contentType: 'terminal' as const };

  it('passes a terminal that this client may use', () => {
    terminal.allowed = true;
    expect(appWindowPolicy.unavailableReason(term)).toBeNull();
  });

  it('names the remedy for a terminal that this client may not use', () => {
    terminal.allowed = false;
    expect(appWindowPolicy.unavailableReason(term)).toBe(
      'only available from the host machine (or with remote_shell enabled)',
    );
  });

  it('passes every other tab, whatever the terminal rule says', () => {
    terminal.allowed = false;
    expect(appWindowPolicy.unavailableReason({ id: 'f', title: 'Files', contentType: 'files' })).toBeNull();
  });
});
