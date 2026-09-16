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

/** The order a chord prints its modifiers in, whatever order it declares them. */
const MODIFIER_ORDER: ShortcutAction['modifiers'] = ['ctrl', 'shift', 'alt', 'meta'];

const MODIFIER_LABEL: Record<ShortcutAction['modifiers'][number], string> = {
  ctrl: 'Ctrl',
  shift: 'Shift',
  alt: 'Alt',
  meta: 'Meta',
};

/**
 * The printed chord for `action` in `shortcuts`, or null when no chord
 * carries it.
 *
 * A hint reads the binding table instead of spelling the keys at the call
 * site. A hand-written hint goes stale the first time a binding moves.
 */
export function chordLabel(action: string, shortcuts: readonly ShortcutAction[]): string | null {
  const shortcut = shortcuts.find((s) => s.action === action);
  if (!shortcut) return null;
  const modifiers = MODIFIER_ORDER.filter((m) => shortcut.modifiers.includes(m)).map(
    (m) => MODIFIER_LABEL[m],
  );
  // Single characters print uppercase ('o' prints 'O'); named keys stay exact.
  const key = shortcut.key.length === 1 ? shortcut.key.toUpperCase() : shortcut.key;
  return [...modifiers, key].join('+');
}

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
