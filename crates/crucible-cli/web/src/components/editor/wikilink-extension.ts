/**
 * CodeMirror wikilink navigation for kiln notes.
 *
 * Decorates `[[wikilinks]]` as follow-able links (`.cm-wikilink` with a
 * `data-note` attribute — which also opts them into the app-wide hover
 * preview), and wires two follow gestures:
 *   - Ctrl/Cmd + Click on a link
 *   - Mod-Enter with the cursor inside a link
 */
import {
  EditorView,
  ViewPlugin,
  ViewUpdate,
  Decoration,
  DecorationSet,
  MatchDecorator,
  keymap,
} from '@codemirror/view';
import { Prec, type EditorState, type Extension } from '@codemirror/state';
import { parseWikilinkInner } from '@/lib/markdown';

const WIKILINK_RE = /\[\[([^\[\]\n]+)\]\]/g;

const wikilinkDecorator = new MatchDecorator({
  regexp: WIKILINK_RE,
  decoration: (match) => {
    const { target } = parseWikilinkInner(match[1]);
    return Decoration.mark({
      class: 'cm-wikilink',
      attributes: { 'data-note': target },
    });
  },
});

const wikilinkHighlighter = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;

    constructor(view: EditorView) {
      this.decorations = wikilinkDecorator.createDeco(view);
    }

    update(update: ViewUpdate) {
      this.decorations = wikilinkDecorator.updateDeco(update, this.decorations);
    }
  },
  { decorations: (v) => v.decorations },
);

// Ember-tinted pill rather than an underline: reads as "knowledge link",
// stays distinct from markdown's own [link](url) styling, and the underline
// only appears on hover as the follow affordance.
const wikilinkTheme = EditorView.baseTheme({
  '.cm-wikilink': {
    color: 'var(--color-primary, #e0653a)',
    backgroundColor: 'color-mix(in srgb, var(--color-primary, #e0653a) 10%, transparent)',
    borderRadius: '3px',
    cursor: 'pointer',
  },
  '.cm-wikilink:hover': {
    textDecoration: 'underline',
    textUnderlineOffset: '3px',
    backgroundColor: 'color-mix(in srgb, var(--color-primary, #e0653a) 18%, transparent)',
  },
});

/** The wikilink target under `pos`, or `null` when the cursor isn't in one. */
export function wikilinkTargetAt(state: EditorState, pos: number): string | null {
  const line = state.doc.lineAt(pos);
  for (const match of line.text.matchAll(WIKILINK_RE)) {
    const from = line.from + (match.index ?? 0);
    const to = from + match[0].length;
    if (pos >= from && pos <= to) {
      return parseWikilinkInner(match[1]).target;
    }
  }
  return null;
}

/** Keymap command: follow the wikilink under the cursor. */
export function followWikilinkAtCursor(
  onFollow: (target: string) => void,
): (view: EditorView) => boolean {
  return (view) => {
    const target = wikilinkTargetAt(view.state, view.state.selection.main.head);
    if (!target) return false;
    onFollow(target);
    return true;
  };
}

/**
 * Full wikilink navigation bundle: decorations, styling, Ctrl/Cmd+Click,
 * and the Mod-Enter follow binding.
 */
export function wikilinkNavigation(onFollow: (target: string) => void): Extension {
  return [
    wikilinkHighlighter,
    wikilinkTheme,
    EditorView.domEventHandlers({
      mousedown: (event, _view) => {
        if (!(event.ctrlKey || event.metaKey)) return false;
        const el = (event.target as Element | null)?.closest?.('.cm-wikilink');
        const target = el?.getAttribute('data-note');
        if (!target) return false;
        event.preventDefault();
        onFollow(target);
        return true;
      },
    }),
    // defaultKeymap binds Mod-Enter to insertBlankLine; Prec.high makes the
    // follow command win when the cursor is inside a link. It returns false
    // outside links, so insertBlankLine still runs everywhere else.
    Prec.high(keymap.of([{ key: 'Mod-Enter', run: followWikilinkAtCursor(onFollow) }])),
  ];
}
