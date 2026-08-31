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
import { parseWikilinkInner, wikilinkRe } from '@/lib/markdown';
import { inCodeContext } from './md-context';

const wikilinkDecorator = new MatchDecorator({
  regexp: wikilinkRe(),
  // `decorate` (not `decoration`) so code contexts can be SKIPPED — TOML
  // `[[mcp.upstreams]]` array-of-tables headers in fenced blocks are code,
  // not knowledge links.
  decorate: (add, from, to, match, view) => {
    if (inCodeContext(view.state, from)) return;
    const { target } = parseWikilinkInner(match[1]);
    add(
      from,
      to,
      Decoration.mark({
        class: 'cm-wikilink',
        attributes: { 'data-note': target },
      }),
    );
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

/**
 * Ember-tinted pill rather than an underline: reads as "knowledge link",
 * stays distinct from markdown's own [link](url) styling, and the underline
 * only appears on hover as the follow affordance.
 *
 * `.cm-wikilink span` beside `.cm-wikilink`, and that second selector is the
 * whole fix. The mark decoration does not own the text it wraps: the markdown
 * language reads `[[Note]]` as a link label and highlights the brackets and
 * the name, so the DOM CodeMirror builds is
 *
 *     <span class="cm-wikilink">[<span class="ͼ10 ͼ13">[</span>…
 *
 * and the syntax class sits on a DESCENDANT. A descendant's own `color` beats
 * an ancestor's whatever the extension order says, so `.cm-wikilink` alone
 * left every wikilink painted in the syntax theme's string colour — One Dark's
 * green #98c379 in the dark theme, measured, not the ember asked for here.
 *
 * NOT a precedence problem, and a precedence fix does not touch it: this stays
 * a `baseTheme` (`Prec.lowest`) and still wins, because `.cm-wikilink span`
 * outranks a bare `.ͼ10` on SPECIFICITY. Promoting it to `EditorView.theme`
 * changes no computed colour — checked by reverting it and watching the tests
 * stay green. `!important` would be wrong twice over: unnecessary here, and it
 * would outrank the review and search decorations that legitimately recolour
 * a span inside a link.
 */
const wikilinkTheme = EditorView.baseTheme({
  '.cm-wikilink, .cm-wikilink span': {
    color: 'var(--color-primary)',
  },
  '.cm-wikilink': {
    backgroundColor: 'color-mix(in srgb, var(--color-primary) 10%, transparent)',
    borderRadius: '3px',
    cursor: 'pointer',
  },
  '.cm-wikilink:hover': {
    textDecoration: 'underline',
    textUnderlineOffset: '3px',
    backgroundColor: 'color-mix(in srgb, var(--color-primary) 18%, transparent)',
  },
});

/** The wikilink target under `pos`, or `null` when the cursor isn't in one. */
export function wikilinkTargetAt(state: EditorState, pos: number): string | null {
  const line = state.doc.lineAt(pos);
  for (const match of line.text.matchAll(wikilinkRe())) {
    const from = line.from + (match.index ?? 0);
    const to = from + match[0].length;
    if (pos >= from && pos <= to) {
      if (inCodeContext(state, from)) return null;
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
