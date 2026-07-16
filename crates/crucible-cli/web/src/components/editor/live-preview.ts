/**
 * Obsidian-style live preview for markdown buffers.
 *
 * Everything renders styled prose — headings sized, bold bold, inline code
 * as a mono chip, wikilinks as pills showing their display text — with the
 * markdown syntax characters hidden. The ONE construct the cursor is in
 * reveals its raw source (backticks, asterisks, brackets) so it can be
 * edited; move the cursor out and it snaps back to styled. Source mode
 * (the mono, everything-raw flow) stays available as a toggle.
 *
 * Implementation: a ViewPlugin walks the lezer syntax tree over the
 * visible ranges on every doc/selection/viewport change. Per construct it
 * adds a styling mark over the content and — unless a selection touches
 * the construct (whole line, for headings) — replace decorations that hide
 * the syntax marks. Wikilinks are not lezer nodes; a regex pass hides
 * their brackets (and `target|` for aliased links) with the same
 * cursor-reveal rule, stacking with the pill styling from
 * wikilink-extension.
 */
import {
  EditorView,
  Decoration,
  type DecorationSet,
  ViewPlugin,
  type ViewUpdate,
} from '@codemirror/view';
import { type EditorState, type Extension, type Range } from '@codemirror/state';
import { syntaxTree } from '@codemirror/language';
import type { SyntaxNode } from '@lezer/common';

const HIDE = Decoration.replace({});

const CONTENT_MARKS: Record<string, Decoration> = {
  StrongEmphasis: Decoration.mark({ class: 'cm-lp-strong' }),
  Emphasis: Decoration.mark({ class: 'cm-lp-em' }),
  Strikethrough: Decoration.mark({ class: 'cm-lp-strike' }),
  InlineCode: Decoration.mark({ class: 'cm-lp-code' }),
  Link: Decoration.mark({ class: 'cm-lp-link' }),
};

const HEADING_LEVELS: Record<string, number> = {
  ATXHeading1: 1,
  ATXHeading2: 2,
  ATXHeading3: 3,
  ATXHeading4: 4,
  ATXHeading5: 5,
  ATXHeading6: 6,
};

/** Marks hidden inside each construct when the cursor is elsewhere. */
const MARKS_TO_HIDE: Record<string, string[]> = {
  StrongEmphasis: ['EmphasisMark'],
  Emphasis: ['EmphasisMark'],
  Strikethrough: ['StrikethroughMark'],
  InlineCode: ['CodeMark'],
  Link: ['LinkMark', 'URL', 'LinkTitle'],
};

function selectionTouches(state: EditorState, from: number, to: number): boolean {
  return state.selection.ranges.some((r) => r.from <= to && r.to >= from);
}

function hideChildMarks(
  node: SyntaxNode,
  names: string[],
  out: Range<Decoration>[],
): void {
  for (const name of names) {
    for (const child of node.getChildren(name)) {
      out.push(HIDE.range(child.from, child.to));
    }
  }
}

const WIKILINK_RE = /\[\[([^[\]\n]+)\]\]/g;

function buildDecorations(view: EditorView): DecorationSet {
  const { state } = view;
  const doc = state.doc;
  const decorations: Range<Decoration>[] = [];

  for (const { from, to } of view.visibleRanges) {
    syntaxTree(state).iterate({
      from,
      to,
      enter: (nodeRef) => {
        const name = nodeRef.name;

        const headingLevel = HEADING_LEVELS[name];
        if (headingLevel) {
          decorations.push(
            Decoration.mark({ class: `cm-lp-h${headingLevel}` }).range(
              nodeRef.from,
              nodeRef.to,
            ),
          );
          // Headings reveal on the whole line — the `#` marks come back as
          // soon as the cursor lands anywhere in the heading.
          const line = doc.lineAt(nodeRef.from);
          if (!selectionTouches(state, line.from, line.to)) {
            const node = nodeRef.node;
            for (const mark of node.getChildren('HeaderMark')) {
              // Swallow the space after `#` too, so the content sits flush.
              const end =
                mark.to < doc.length && doc.sliceString(mark.to, mark.to + 1) === ' '
                  ? mark.to + 1
                  : mark.to;
              decorations.push(HIDE.range(mark.from, end));
            }
          }
          return;
        }

        const contentMark = CONTENT_MARKS[name];
        if (contentMark) {
          // Lezer parses bare `[text]` (and the inside of `[[wikilinks]]`)
          // as a Link node with no URL — those aren't links; leave them raw
          // for the wikilink pass / plain text.
          if (name === 'Link' && nodeRef.node.getChildren('URL').length === 0) {
            return;
          }
          decorations.push(contentMark.range(nodeRef.from, nodeRef.to));
          if (!selectionTouches(state, nodeRef.from, nodeRef.to)) {
            hideChildMarks(nodeRef.node, MARKS_TO_HIDE[name] ?? [], decorations);
          }
          return;
        }

        if (name === 'FencedCode' || name === 'CodeBlock') {
          const first = doc.lineAt(nodeRef.from).number;
          const last = doc.lineAt(nodeRef.to).number;
          for (let n = first; n <= last; n++) {
            decorations.push(
              Decoration.line({ class: 'cm-lp-codeblock' }).range(doc.line(n).from),
            );
          }
          return;
        }

        if (name === 'Blockquote') {
          const first = doc.lineAt(nodeRef.from).number;
          const last = doc.lineAt(nodeRef.to).number;
          for (let n = first; n <= last; n++) {
            decorations.push(
              Decoration.line({ class: 'cm-lp-quote' }).range(doc.line(n).from),
            );
          }
          return;
        }

        if (name === 'ListMark') {
          decorations.push(
            Decoration.mark({ class: 'cm-lp-bullet' }).range(nodeRef.from, nodeRef.to),
          );
        }
      },
    });

    // Wikilinks are Crucible syntax, not lezer nodes: hide `[[`/`]]` (and
    // the `target|` half of aliased links) unless the cursor is inside.
    const text = doc.sliceString(from, to);
    WIKILINK_RE.lastIndex = 0;
    let m: RegExpExecArray | null;
    while ((m = WIKILINK_RE.exec(text))) {
      const start = from + m.index;
      const end = start + m[0].length;
      if (selectionTouches(state, start, end)) continue;
      const pipe = m[1].indexOf('|');
      if (pipe !== -1) {
        decorations.push(HIDE.range(start, start + 2 + pipe + 1));
      } else {
        decorations.push(HIDE.range(start, start + 2));
      }
      decorations.push(HIDE.range(end - 2, end));
    }
  }

  return Decoration.set(decorations, true);
}

const livePreviewPlugin = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;

    constructor(view: EditorView) {
      this.decorations = buildDecorations(view);
    }

    update(update: ViewUpdate) {
      if (update.docChanged || update.selectionSet || update.viewportChanged) {
        this.decorations = buildDecorations(update.view);
      }
    }
  },
  { decorations: (v) => v.decorations },
);

/** Prose-first typography; syntax colors still come from the highlighter. */
const livePreviewTheme = EditorView.baseTheme({
  '&.cm-lp .cm-content': {
    fontFamily: "'IBM Plex Sans', system-ui, sans-serif",
    fontSize: '15px',
    lineHeight: '1.6',
  },
  '.cm-lp-h1': { fontSize: '1.7em', fontWeight: '700' },
  '.cm-lp-h2': { fontSize: '1.45em', fontWeight: '700' },
  '.cm-lp-h3': { fontSize: '1.25em', fontWeight: '700' },
  '.cm-lp-h4': { fontSize: '1.1em', fontWeight: '700' },
  '.cm-lp-h5': { fontSize: '1em', fontWeight: '700' },
  '.cm-lp-h6': { fontSize: '1em', fontWeight: '600', opacity: '0.8' },
  '.cm-lp-strong': { fontWeight: '700' },
  '.cm-lp-em': { fontStyle: 'italic' },
  '.cm-lp-strike': { textDecoration: 'line-through', opacity: '0.75' },
  '.cm-lp-code': {
    fontFamily: "'IBM Plex Mono', ui-monospace, monospace",
    fontSize: '0.88em',
    background: 'rgba(255, 255, 255, 0.08)',
    borderRadius: '3px',
    padding: '0.5px 4px',
  },
  '.cm-lp-link': { color: 'var(--color-primary, #e0653a)' },
  '.cm-lp-codeblock': {
    fontFamily: "'IBM Plex Mono', ui-monospace, monospace",
    fontSize: '0.88em',
    background: 'rgba(255, 255, 255, 0.04)',
  },
  '.cm-lp-quote': {
    borderLeft: '2px solid rgba(224, 101, 58, 0.5)',
    color: 'rgba(232, 230, 227, 0.75)',
    fontStyle: 'italic',
  },
  '.cm-lp-bullet': { color: 'var(--color-primary, #e0653a)' },
});

export function livePreview(): Extension {
  return [
    // Scope the prose font to live-preview editors only.
    EditorView.editorAttributes.of({ class: 'cm-lp' }),
    livePreviewPlugin,
    livePreviewTheme,
  ];
}
