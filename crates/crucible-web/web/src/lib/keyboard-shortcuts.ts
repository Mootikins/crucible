export interface ShortcutAction {
  key: string;           // KeyboardEvent.key value
  modifiers: ('ctrl' | 'shift' | 'alt' | 'meta')[];
  action: string;        // action identifier
  description: string;   // human-readable description
}

export const DEFAULT_SHORTCUTS: ShortcutAction[] = [
  { key: 'w', modifiers: ['ctrl'], action: 'closeActiveTab', description: 'Close active tab' },
  { key: 'Tab', modifiers: ['ctrl'], action: 'nextTab', description: 'Next tab' },
  { key: '\\', modifiers: ['ctrl'], action: 'splitVertical', description: 'Split pane vertically' },
  { key: 'b', modifiers: ['ctrl'], action: 'toggleLeftPanel', description: 'Toggle left panel' },
];

// Note: Ctrl+W may be intercepted by browser. Works in PWA/Electron contexts.

export function matchShortcut(e: KeyboardEvent, shortcuts: ShortcutAction[] = DEFAULT_SHORTCUTS): string | null {
  for (const shortcut of shortcuts) {
    const ctrlMatch = shortcut.modifiers.includes('ctrl') ? (e.ctrlKey || e.metaKey) : (!e.ctrlKey && !e.metaKey);
    const shiftMatch = shortcut.modifiers.includes('shift') ? e.shiftKey : !e.shiftKey;
    const altMatch = shortcut.modifiers.includes('alt') ? e.altKey : !e.altKey;
    if (ctrlMatch && shiftMatch && altMatch && e.key === shortcut.key) {
      return shortcut.action;
    }
  }
  return null;
}
