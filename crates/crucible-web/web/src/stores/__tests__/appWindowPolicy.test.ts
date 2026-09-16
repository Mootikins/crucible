import { describe, it, expect, vi, afterEach } from 'vitest';
import { appWindowPolicy } from '@/stores/windowStore';
import { DEFAULT_SHORTCUTS } from '@/lib/keyboard-shortcuts';
import { LAYOUT_ACTIONS } from '@/windowing/shortcuts';

const keydown = () => new KeyboardEvent('keydown');

afterEach(() => vi.restoreAllMocks());

describe('appWindowPolicy.onShortcut', () => {
  it('consumes every app chord in its table, so the browser default stays off', () => {
    const appActions = DEFAULT_SHORTCUTS.map((s) => s.action).filter((a) => !LAYOUT_ACTIONS.has(a));
    vi.spyOn(window, 'dispatchEvent').mockReturnValue(true);
    for (const action of appActions) {
      expect(appWindowPolicy.onShortcut(action, keydown()), action).toBe(true);
    }
  });

  it('asks for a clear through the app event', () => {
    const seen: string[] = [];
    const listener = (e: Event) => seen.push(e.type);
    window.addEventListener('crucible:clear-chat', listener);
    appWindowPolicy.onShortcut('clearChat', keydown());
    window.removeEventListener('crucible:clear-chat', listener);
    expect(seen).toEqual(['crucible:clear-chat']);
  });
});
