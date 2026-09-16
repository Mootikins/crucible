/** One chord and the action it names. */
export interface ShortcutAction {
  /** A `KeyboardEvent.key` value. */
  key: string;
  modifiers: ('ctrl' | 'shift' | 'alt' | 'meta')[];
  /** The action identifier. */
  action: string;
  /** A description that a person reads. */
  description: string;
}

/** The chords the layout owns. An app puts these first in its table. */
export const LAYOUT_SHORTCUTS: ShortcutAction[] = [
  { key: 'w', modifiers: ['ctrl'], action: 'closeActiveTab', description: 'Close active tab' },
  { key: 'Tab', modifiers: ['ctrl'], action: 'nextTab', description: 'Next tab' },
  { key: '\\', modifiers: ['ctrl'], action: 'splitVertical', description: 'Split pane vertically' },
  // Adjacent to the split chord, which keeps Ctrl+\ where the muscle memory
  // already is. Both are "rearrange the panes", so they read as a pair.
  { key: '\\', modifiers: ['ctrl', 'shift'], action: 'swapSidePanels', description: 'Swap side panels' },
  { key: 'b', modifiers: ['ctrl'], action: 'toggleLeftPanel', description: 'Toggle left panel' },
  { key: 'e', modifiers: ['ctrl', 'shift'], action: 'toggleRightPanel', description: 'Toggle right panel' },
];

/** The action names in `LAYOUT_SHORTCUTS`. */
export const LAYOUT_ACTIONS: ReadonlySet<string> = new Set(LAYOUT_SHORTCUTS.map((s) => s.action));

/** The action of the first chord in `shortcuts` that matches the event, or null. */
export function matchShortcut(e: KeyboardEvent, shortcuts: readonly ShortcutAction[]): string | null {
  for (const shortcut of shortcuts) {
    const ctrlMatch = shortcut.modifiers.includes('ctrl') ? (e.ctrlKey || e.metaKey) : (!e.ctrlKey && !e.metaKey);
    const shiftMatch = shortcut.modifiers.includes('shift') ? e.shiftKey : !e.shiftKey;
    const altMatch = shortcut.modifiers.includes('alt') ? e.altKey : !e.altKey;
    // Shift changes e.key's case for character keys ('n' arrives as 'N'), so
    // single-char keys compare case-insensitively; named keys (Tab, Escape)
    // stay exact.
    const keyMatch =
      shortcut.key.length === 1
        ? e.key.toLowerCase() === shortcut.key.toLowerCase()
        : e.key === shortcut.key;
    if (ctrlMatch && shiftMatch && altMatch && keyMatch) {
      return shortcut.action;
    }
  }
  return null;
}
