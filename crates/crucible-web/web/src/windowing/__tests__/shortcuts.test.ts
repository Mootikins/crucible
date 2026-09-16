import { describe, it, expect } from 'vitest';
import { chordLabel, LAYOUT_SHORTCUTS, type ShortcutAction } from '@/windowing/shortcuts';

describe('chordLabel', () => {
  it('prints the chord that the table gives the action', () => {
    expect(chordLabel('swapSidePanels', LAYOUT_SHORTCUTS)).toBe('Ctrl+Shift+\\');
    expect(chordLabel('toggleLeftPanel', LAYOUT_SHORTCUTS)).toBe('Ctrl+B');
  });

  it('prints the modifiers in a fixed order, whatever order the table gives', () => {
    const table: ShortcutAction[] = [
      { key: 'x', modifiers: ['meta', 'alt', 'shift', 'ctrl'], action: 'a', description: '' },
    ];
    expect(chordLabel('a', table)).toBe('Ctrl+Shift+Alt+Meta+X');
  });

  it('keeps a named key exact', () => {
    const table: ShortcutAction[] = [{ key: 'Escape', modifiers: [], action: 'a', description: '' }];
    expect(chordLabel('a', table)).toBe('Escape');
  });

  it('returns null for an action that the table does not bind', () => {
    expect(chordLabel('nothing', LAYOUT_SHORTCUTS)).toBeNull();
  });
});
