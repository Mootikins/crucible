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
  { key: 'Tab', modifiers: ['shift'], action: 'cycleMode', description: 'Cycle chat mode (Normal → Plan → Auto)' },
  // Command palette — Ctrl+P / Cmd+P (browser print intercepted with preventDefault)
  { key: 'p', modifiers: ['ctrl'], action: 'openCommandPalette', description: 'Open command palette' },
  // Chat focus — Ctrl+/ focuses chat input
  { key: '/', modifiers: ['ctrl'], action: 'focusChatInput', description: 'Focus chat input' },
  // Overlay management — Escape closes active overlay
  { key: 'Escape', modifiers: [], action: 'closeOverlay', description: 'Close active overlay' },
  // Session management
  { key: 'n', modifiers: ['ctrl', 'shift'], action: 'newSession', description: 'New chat session' },
  // Panel toggles
  { key: 'e', modifiers: ['ctrl', 'shift'], action: 'toggleRightPanel', description: 'Toggle right panel' },
  { key: 'b', modifiers: ['ctrl', 'shift'], action: 'toggleBottomPanel', description: 'Toggle bottom panel' },
  // Chat actions
  { key: 'k', modifiers: ['ctrl'], action: 'clearChat', description: 'Clear chat' },
  // Thinking display toggle — Ctrl+T / Cmd+T
  { key: 't', modifiers: ['ctrl'], action: 'toggleThinking', description: 'Toggle thinking display visibility' },
];

// Browser conflicts:
// - Ctrl+W: Close tab (browser default) — works in PWA/Electron, blocked in regular browser
// - Ctrl+P: Print dialog (browser default) — preventDefault() in handler blocks it
// - Ctrl+T: New tab (browser default) — preventDefault() in handler blocks it
// - Escape: May close fullscreen or cancel operations — handled per context

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
