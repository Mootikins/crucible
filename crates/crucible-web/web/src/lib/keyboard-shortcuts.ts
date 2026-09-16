import { LAYOUT_SHORTCUTS, type ShortcutAction } from '@/windowing/shortcuts';

/** The chords the app owns. The layout chords come from the windowing core. */
const APP_SHORTCUTS: ShortcutAction[] = [
  { key: 'Tab', modifiers: ['shift'], action: 'cycleMode', description: 'Cycle chat mode (Normal → Plan → Auto)' },
  // Command palette — Ctrl+P / Cmd+P (browser print intercepted with preventDefault)
  { key: 'p', modifiers: ['ctrl'], action: 'openCommandPalette', description: 'Open command palette' },
  // Note quick switcher — Ctrl+O (Obsidian's binding; browser open-file
  // dialog intercepted with preventDefault). Opens the palette pre-scoped
  // to notes so note-open doesn't share Ctrl+P's mixed results.
  { key: 'o', modifiers: ['ctrl'], action: 'openNoteSwitcher', description: 'Open note (quick switcher)' },
  // Content search — Ctrl+Shift+F (VSCode's "search in files" binding)
  { key: 'f', modifiers: ['ctrl', 'shift'], action: 'openSearch', description: 'Search notes, files & sessions' },
  // Chat focus — Ctrl+/ focuses chat input
  { key: '/', modifiers: ['ctrl'], action: 'focusChatInput', description: 'Focus chat input' },
  // Overlay management — Escape closes active overlay
  { key: 'Escape', modifiers: [], action: 'closeOverlay', description: 'Close active overlay' },
  // Session management
  { key: 'n', modifiers: ['ctrl', 'shift'], action: 'newSession', description: 'New chat session' },
  // Chat actions
  { key: 'k', modifiers: ['ctrl'], action: 'clearChat', description: 'Clear chat' },
  // Thinking display toggle — Ctrl+T / Cmd+T
  { key: 't', modifiers: ['ctrl'], action: 'toggleThinking', description: 'Toggle thinking display visibility' },
];

/** Every chord, in match order: the layout first, then the app. */
export const DEFAULT_SHORTCUTS: ShortcutAction[] = [...LAYOUT_SHORTCUTS, ...APP_SHORTCUTS];

// Browser conflicts:
// - Ctrl+W: Close tab (browser default) — works in PWA/Electron, blocked in regular browser
// - Ctrl+P: Print dialog (browser default) — preventDefault() in handler blocks it
// - Ctrl+T (new tab) and Ctrl+Shift+N (incognito): RESERVED by Chrome/Firefox —
//   the keydown never reaches page JS in a regular browser tab, so these only
//   work in PWA/Electron windows. The command palette entries are the
//   universal path for toggle-thinking and new-session.
// - Escape: May close fullscreen or cancel operations — handled per context

/** The order a chord prints its modifiers in, whatever order it declares them. */
const MODIFIER_ORDER: ShortcutAction['modifiers'] = ['ctrl', 'shift', 'alt', 'meta'];

const MODIFIER_LABEL: Record<ShortcutAction['modifiers'][number], string> = {
  ctrl: 'Ctrl',
  shift: 'Shift',
  alt: 'Alt',
  meta: 'Meta',
};

/**
 * The printed chord for an action, or null when no binding carries it.
 *
 * A hint reads the binding table instead of spelling the keys at the call
 * site. A hand-written hint goes stale the first time a binding moves, and a
 * hint that names keys the app does not listen for is worse than no hint.
 */
export function shortcutLabel(
  action: string,
  shortcuts: ShortcutAction[] = DEFAULT_SHORTCUTS,
): string | null {
  const shortcut = shortcuts.find((s) => s.action === action);
  if (!shortcut) return null;
  const modifiers = MODIFIER_ORDER.filter((m) => shortcut.modifiers.includes(m)).map(
    (m) => MODIFIER_LABEL[m],
  );
  // Single characters print uppercase ('o' → 'O'); named keys stay exact.
  const key = shortcut.key.length === 1 ? shortcut.key.toUpperCase() : shortcut.key;
  return [...modifiers, key].join('+');
}
