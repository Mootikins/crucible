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
  WidgetType,
} from '@codemirror/view';
import {
  Annotation,
  EditorState,
  Facet,
  StateField,
  type Extension,
  type Range,
} from '@codemirror/state';
import { syntaxTree } from '@codemirror/language';
import type { SyntaxNode } from '@lezer/common';
import { renderMarkdown, wikilinkRe, rawImageUrl, sanitizeDocHtml } from '@/lib/markdown';
import { extractFrontmatterBlock, renderFrontmatterCardHtml } from '@/lib/frontmatter';
import { resolveCalloutKind } from '@/lib/callouts';
import { formatTableLines } from '@/lib/table-format';
import { inCodeOrTableContext } from './md-context';

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

const WIKILINK_RE = wikilinkRe();

/** `> [!type]` head on a blockquote's first line — marks it as a callout. */
const CALLOUT_HEAD_LINE_RE = /^\s*>\s*\[!([a-zA-Z]+)\]/;

/** `![alt](url)` — captures alt (1) and url (2). */
const IMAGE_RE = /^!\[([^\]]*)\]\(([^)\s]+)(?:\s+"[^"]*")?\)$/;

/** The document's directory, used to resolve relative image srcs. Injected
 * per-editor by {@link livePreview} so images load the same way the reading
 * view does (through the raw project-file endpoint). */
const baseDirFacet = Facet.define<string, string>({
  combine: (values) => values[0] ?? '',
});

/** A GFM task-list checkbox rendered in place of a `[ ]`/`[x]` marker.
 * Clicking toggles the marker in the source (so both states are editable);
 * the cursor entering the marker reveals the raw brackets. */
class CheckboxWidget extends WidgetType {
  constructor(readonly checked: boolean) {
    super();
  }

  override eq(other: CheckboxWidget): boolean {
    return other.checked === this.checked;
  }

  override toDOM(view: EditorView): HTMLElement {
    const box = document.createElement('span');
    box.className = `cm-lp-checkbox${this.checked ? ' is-checked' : ''}`;
    box.setAttribute('role', 'checkbox');
    box.setAttribute('aria-checked', String(this.checked));
    box.addEventListener('mousedown', (e) => {
      e.preventDefault();
      const from = view.posAtDOM(box);
      const marker = view.state.doc.sliceString(from, from + 3);
      // Toggle the char between the brackets (`[ ]` ↔ `[x]`).
      if (/^\[[ xX]\]$/.test(marker)) {
        view.dispatch({
          changes: { from: from + 1, to: from + 2, insert: marker[1] === ' ' ? 'x' : ' ' },
        });
      }
    });
    return box;
  }

  override ignoreEvent(): boolean {
    return false;
  }
}

/** An inline image rendered in place of `![alt](url)` (or a badge's
 * `[![alt](img)](link)`), snapping back to source when the cursor enters. */
class ImageWidget extends WidgetType {
  constructor(
    readonly url: string,
    readonly alt: string,
  ) {
    super();
  }

  override eq(other: ImageWidget): boolean {
    return other.url === this.url && other.alt === this.alt;
  }

  override toDOM(view: EditorView): HTMLElement {
    const img = document.createElement('img');
    img.className = 'cm-lp-img';
    img.src = this.url;
    img.alt = this.alt;
    // Click drops the cursor into the source so the markup can be edited.
    img.addEventListener('mousedown', (e) => {
      e.preventDefault();
      const pos = view.posAtDOM(img);
      view.dispatch({ selection: { anchor: pos } });
      view.focus();
    });
    return img;
  }

  override ignoreEvent(): boolean {
    return false;
  }
}

function buildDecorations(view: EditorView): DecorationSet {
  const { state } = view;
  const doc = state.doc;
  const decorations: Range<Decoration>[] = [];

  // TOML frontmatter (no lezer node): mono/dim line styling for the revealed
  // (cursor-inside) state; when the cursor is out, the block widget covers it.
  const toml = tomlFrontmatterRange(state);
  if (toml) {
    const last = doc.lineAt(toml.to).number;
    for (let n = 1; n <= last; n++) {
      decorations.push(
        Decoration.line({ class: 'cm-lp-frontmatter' }).range(doc.line(n).from),
      );
    }
  }

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

        if (name === 'Image') {
          // `![alt](url)` renders as an inline <img>; the cursor entering the
          // markup reveals the source for editing. Badges (`[![](img)](link)`)
          // parse as this Image nested in a Link — the Link handler above
          // already hides the outer `[…](link)`, so only the image shows.
          if (selectionTouches(state, nodeRef.from, nodeRef.to)) return false;
          const m = IMAGE_RE.exec(doc.sliceString(nodeRef.from, nodeRef.to));
          if (!m) return false;
          const url = rawImageUrl(m[2], state.facet(baseDirFacet) || undefined);
          if (!url) return false;
          decorations.push(
            Decoration.replace({ widget: new ImageWidget(url, m[1]) }).range(
              nodeRef.from,
              nodeRef.to,
            ),
          );
          return false;
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

        if (name === 'Table') {
          // Revealed table source (cursor inside): monospace lines so pipe
          // alignment is real, and NO inline styling/mark-hiding — hidden
          // syntax chars would make visual columns lie about source columns.
          // Rendered tables are tableField's block widget; these decorations
          // are inert then.
          if (selectionTouches(state, nodeRef.from, nodeRef.to)) {
            const first = doc.lineAt(nodeRef.from).number;
            const endLine = doc.lineAt(nodeRef.to);
            const last = endLine.from === nodeRef.to ? endLine.number - 1 : endLine.number;
            for (let n = first; n <= last; n++) {
              decorations.push(
                Decoration.line({ class: 'cm-lp-tablesrc' }).range(doc.line(n).from),
              );
            }
          }
          return false;
        }

        if (name === 'Blockquote') {
          const firstLine = doc.lineAt(nodeRef.from);
          const last = doc.lineAt(nodeRef.to).number;
          // `> [!type]` heads style the whole quote as that callout variant
          // (colors shared with the reading-mode CSS); plain quotes keep the
          // italic treatment. Like tables, these line styles only matter for
          // the revealed source (cursor inside) — rendered callouts are
          // calloutField's block widget, which makes them inert.
          const head = CALLOUT_HEAD_LINE_RE.exec(firstLine.text);
          if (head) {
            const kind = resolveCalloutKind(head[1]);
            for (let n = firstLine.number; n <= last; n++) {
              const title = n === firstLine.number ? ' cm-lp-callout-title' : '';
              decorations.push(
                Decoration.line({
                  class: `cm-lp-callout cm-lp-callout-${kind}${title}`,
                }).range(doc.line(n).from),
              );
            }
            return;
          }
          for (let n = firstLine.number; n <= last; n++) {
            decorations.push(
              Decoration.line({ class: 'cm-lp-quote' }).range(doc.line(n).from),
            );
          }
          return;
        }

        // YAML frontmatter (via yamlFrontmatter's outer parser): keep it
        // raw — mono, dimmed, yaml-highlighted — until a Properties-style
        // UI replaces it. Skip descending so markdown styling never
        // applies inside.
        if (name === 'Frontmatter') {
          const first = doc.lineAt(nodeRef.from).number;
          // The node ends AT the next line's start — don't style that line.
          const endLine = doc.lineAt(nodeRef.to);
          const last = endLine.from === nodeRef.to ? endLine.number - 1 : endLine.number;
          for (let n = first; n <= last; n++) {
            decorations.push(
              Decoration.line({ class: 'cm-lp-frontmatter' }).range(doc.line(n).from),
            );
          }
          return false;
        }

        if (name === 'ListMark') {
          decorations.push(
            Decoration.mark({ class: 'cm-lp-bullet' }).range(nodeRef.from, nodeRef.to),
          );
          return;
        }

        if (name === 'TaskMarker') {
          // `[ ]`/`[x]` → a clickable checkbox, unless the cursor is on it.
          if (selectionTouches(state, nodeRef.from, nodeRef.to)) return;
          const checked = /[xX]/.test(doc.sliceString(nodeRef.from, nodeRef.to));
          decorations.push(
            Decoration.replace({ widget: new CheckboxWidget(checked) }).range(
              nodeRef.from,
              nodeRef.to,
            ),
          );
        }
      },
    });

    // Wikilinks are Crucible syntax, not lezer nodes: hide `[[`/`]]` (and
    // the `target|` half of aliased links) unless the cursor is inside.
    // Skipped in code contexts (TOML `[[table]]` headers are code, not
    // links) and in tables (revealed source must stay character-exact).
    const text = doc.sliceString(from, to);
    WIKILINK_RE.lastIndex = 0;
    let m: RegExpExecArray | null;
    while ((m = WIKILINK_RE.exec(text))) {
      const start = from + m.index;
      const end = start + m[0].length;
      if (selectionTouches(state, start, end)) continue;
      if (inCodeOrTableContext(state, start)) continue;
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

/** A markdown block rendered as real HTML (sanitized through the chat
 * markdown pipeline): tables become `<table>`, callout blockquotes become
 * the `.callout` admonition markup from lib/callouts.ts (icon + colored
 * title row + tinted body), matching reading mode. Clicking it drops the
 * cursor into the source, which reveals the raw markdown for editing —
 * except clicks on a foldable callout's summary row, which keep their
 * native `<details>` toggle. */
type BlockKind = 'table' | 'callout' | 'html' | 'frontmatter';

/** Card HTML for a frontmatter block's raw source (delimiters included);
 * null when the flat parser can't represent it (callers keep raw source). */
function frontmatterCardHtml(raw: string): string | null {
  const block = extractFrontmatterBlock(raw.endsWith('\n') ? raw : `${raw}\n`);
  if (!block?.entries?.length) return null;
  return renderFrontmatterCardHtml(block.entries);
}

/** TOML (`+++`) frontmatter has NO lezer node (yamlFrontmatter only wraps
 * `---`) — detect it at doc start so it still gets the card widget instead
 * of rendering as body text. Returns the replace range (delimiters
 * included, trailing newline excluded). */
function tomlFrontmatterRange(state: EditorState): { from: number; to: number } | null {
  if (!state.doc.sliceString(0, 3).startsWith('+++')) return null;
  const head = state.doc.sliceString(0, Math.min(state.doc.length, 8192));
  const block = extractFrontmatterBlock(head);
  if (!block || block.format !== 'toml') return null;
  const to = head[block.bodyStart - 1] === '\n' ? block.bodyStart - 1 : block.bodyStart;
  return { from: 0, to };
}

class RenderedBlockWidget extends WidgetType {
  constructor(
    readonly source: string,
    readonly kind: BlockKind,
    readonly baseDir: string,
  ) {
    super();
  }

  override eq(other: RenderedBlockWidget): boolean {
    return (
      other.source === this.source &&
      other.kind === this.kind &&
      other.baseDir === this.baseDir
    );
  }

  override toDOM(view: EditorView): HTMLElement {
    const wrap = document.createElement('div');
    wrap.className =
      this.kind === 'table'
        ? 'cm-lp-table'
        : this.kind === 'callout'
          ? 'cm-lp-callout-block'
          : this.kind === 'frontmatter'
            ? 'cm-lp-fm'
            : 'cm-lp-html';
    wrap.setAttribute('data-testid', `lp-${this.kind}`);
    // Tables/callouts are markdown; a raw HTML block is already HTML and only
    // needs sanitizing + relative-image resolution. Both are DOMPurified.
    // Frontmatter renders the shared Properties card (self-escaped).
    // eslint-disable-next-line solid/no-innerhtml -- DOMPurify-sanitized
    wrap.innerHTML =
      this.kind === 'frontmatter'
        ? (frontmatterCardHtml(this.source) ?? '')
        : this.kind === 'html'
          ? sanitizeDocHtml(this.source, this.baseDir || undefined)
          : renderMarkdown(this.source);
    wrap.addEventListener('mousedown', (e) => {
      // Foldable callout title: let the browser's <details> toggle run
      // instead of jumping into the source.
      if ((e.target as HTMLElement).closest?.('summary.callout-title')) return;
      e.preventDefault();
      const pos = view.posAtDOM(wrap);
      view.dispatch({ selection: { anchor: pos } });
      view.focus();
    });
    return wrap;
  }

  override ignoreEvent(e: Event): boolean {
    // Summary clicks are the widget's own business (fold toggle) — CM must
    // not turn them into selection changes that reveal the source.
    return !!(e.target as HTMLElement).closest?.('summary.callout-title');
  }
}

/** Table, an HTML block, YAML frontmatter, or a Blockquote whose first line
 * is a `> [!type]` callout head — each rendered as a block widget. */
function renderedBlockKind(state: EditorState, name: string, from: number): BlockKind | null {
  if (name === 'Table') return 'table';
  if (name === 'HTMLBlock') return 'html';
  if (name === 'Frontmatter') return 'frontmatter';
  if (name !== 'Blockquote') return null;
  const firstLine = state.doc.lineAt(from);
  return CALLOUT_HEAD_LINE_RE.test(firstLine.text) ? 'callout' : null;
}

function buildBlockWidgets(state: EditorState): DecorationSet {
  const decorations: Range<Decoration>[] = [];
  const baseDir = state.facet(baseDirFacet);

  const pushWidget = (kind: BlockKind, from: number, to: number) => {
    // Selection in the block = editing: show the raw source (the whole
    // block at once — no nested widgets inside revealed source).
    // Frontmatter's END boundary is exclusive: the yaml Frontmatter node
    // ends AT the body's first position, where the cursor deliberately
    // starts (CodeMirrorEditor) — an inclusive test would boot every file
    // open into raw yaml.
    const touchTo = kind === 'frontmatter' ? to - 1 : to;
    if (selectionTouches(state, from, touchTo)) return;
    const source = state.doc.sliceString(from, to);
    // Frontmatter the flat parser can't represent stays raw — a wrong card
    // is worse than mono source.
    if (kind === 'frontmatter' && frontmatterCardHtml(source) === null) return;
    decorations.push(
      Decoration.replace({
        widget: new RenderedBlockWidget(source, kind, baseDir),
        block: true,
      }).range(from, to),
    );
  };

  // TOML frontmatter is invisible to the syntax tree — widget it manually.
  const toml = tomlFrontmatterRange(state);
  if (toml) pushWidget('frontmatter', toml.from, toml.to);

  syntaxTree(state).iterate({
    enter: (nodeRef) => {
      const kind = renderedBlockKind(state, nodeRef.name, nodeRef.from);
      if (!kind) return; // descend — plain blockquotes may contain tables
      pushWidget(kind, nodeRef.from, nodeRef.to);
      return false;
    },
  });
  return Decoration.set(decorations, true);
}

/** Marks our own table-formatting transactions so the listener ignores them. */
const tableFormatAnno = Annotation.define<boolean>();

function tableRangeAt(
  state: EditorState,
  pos: number,
): { from: number; to: number } | null {
  for (const side of [-1, 1] as const) {
    let node: SyntaxNode | null = syntaxTree(state).resolveInner(pos, side);
    while (node) {
      if (node.name === 'Table') return { from: node.from, to: node.to };
      node = node.parent;
    }
  }
  return null;
}

/**
 * Auto-align table source whenever the cursor enters or leaves a table:
 * cells are padded so pipes line up (real columns, since revealed table
 * lines render monospace). Formatting on entry gives an aligned table to
 * edit; formatting on exit tidies whatever the edit un-aligned — so the
 * file stays pretty without ever reflowing under the cursor mid-word.
 */
const tableAutoFormat = EditorView.updateListener.of((update) => {
  if (update.docChanged || !update.selectionSet) return;
  if (update.transactions.some((tr) => tr.annotation(tableFormatAnno))) return;
  const prev = tableRangeAt(update.startState, update.startState.selection.main.head);
  const cur = tableRangeAt(update.state, update.state.selection.main.head);
  if (prev && cur && prev.from === cur.from) return; // moving within one table
  const target = cur ?? prev;
  if (!target) return;

  const doc = update.state.doc;
  const firstLine = doc.lineAt(target.from);
  const endLine = doc.lineAt(target.to);
  const lastLine = endLine.from === target.to ? doc.line(endLine.number - 1) : endLine;
  const lines: string[] = [];
  for (let n = firstLine.number; n <= lastLine.number; n++) {
    lines.push(doc.line(n).text);
  }
  const formatted = formatTableLines(lines);
  if (!formatted || formatted.join('\n') === lines.join('\n')) return;

  const view = update.view;
  // Dispatching inside an update listener is illegal — defer a tick, and
  // bail if anything else changed the doc in between.
  queueMicrotask(() => {
    if (view.state.doc !== doc) return;
    view.dispatch({
      changes: { from: firstLine.from, to: lastLine.to, insert: formatted.join('\n') },
      annotations: tableFormatAnno.of(true),
    });
  });
});

/**
 * Vertical motions (vim j/k, arrows — anything built on moveVertically) skip
 * block widgets entirely: from the line above a rendered table/callout the
 * cursor lands on the line below it, so the keyboard can never enter the
 * block to edit it. This filter catches exactly that hop — a selection-only
 * transaction whose head crossed a rendered block starting from an adjacent
 * line and landing on the adjacent line past it — and redirects the head
 * into the block's edge line (same column), which reveals the raw source
 * via blockWidgetField's selection rule.
 */
const blockWidgetCursorEntry = EditorState.transactionFilter.of((tr) => {
  if (tr.docChanged || !tr.selection) return tr;
  const prev = tr.startState.selection.main.head;
  const next = tr.newSelection.main.head;
  if (prev === next) return tr;
  const doc = tr.startState.doc;
  let redirect: number | null = null;
  syntaxTree(tr.startState).iterate({
    enter: (nodeRef) => {
      if (!renderedBlockKind(tr.startState, nodeRef.name, nodeRef.from)) return;
      if (redirect !== null) return false;
      const { from, to } = nodeRef;
      // Only blocks currently rendered as widgets (cursor was outside).
      if (selectionTouches(tr.startState, from, to)) return false;
      const firstLine = doc.lineAt(from);
      // Like Frontmatter, the Table node can end AT the next line's start —
      // its last real line is then the one before.
      const endLine = doc.lineAt(to);
      const lastLine = endLine.from === to ? doc.line(endLine.number - 1) : endLine;
      const prevLine = doc.lineAt(prev);
      const nextLine = doc.lineAt(next);
      const col = prev - prevLine.from;
      if (
        prevLine.number === firstLine.number - 1 &&
        nextLine.number === lastLine.number + 1
      ) {
        redirect = Math.min(firstLine.from + col, firstLine.to);
      } else if (
        prevLine.number === lastLine.number + 1 &&
        nextLine.number === firstLine.number - 1
      ) {
        redirect = Math.min(lastLine.from + col, lastLine.to);
      }
      return false;
    },
  });
  if (redirect === null) return tr;
  const main = tr.newSelection.main;
  return [
    {
      selection: { anchor: main.empty ? redirect : main.anchor, head: redirect },
      scrollIntoView: true,
    },
  ];
});

// Tables/callouts replace whole line blocks, and CM6 forbids block
// decorations from ViewPlugins — they live in a StateField instead.
const blockWidgetField = StateField.define<DecorationSet>({
  create: buildBlockWidgets,
  update(deco, tr) {
    if (tr.docChanged || tr.selection) return buildBlockWidgets(tr.state);
    return deco.map(tr.changes);
  },
  provide: (f) => EditorView.decorations.from(f),
});

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

/** Prose-first typography; syntax colors still come from the highlighter.
 * PARITY CONTRACT: these values mirror PROSE_CLASS (lib/markdown.ts) — the
 * reading view is canonical, and toggling Edit ↔ Preview must not reflow
 * the document's scale. Body 13px/1.6, em-based headings 1.45/1.25/1.1/1 at
 * weight 600, tokened code/table/blockquote surfaces. */
const livePreviewTheme = EditorView.baseTheme({
  '&.cm-lp .cm-content': {
    fontFamily: "'IBM Plex Sans', system-ui, sans-serif",
    fontSize: '13px',
    lineHeight: '1.6',
  },
  // Ink headings, not oneDark's coral markdown-heading color — the reading
  // view renders headings in shell-ink and Edit ↔ Preview must agree. The
  // descendant reset covers the highlighter's nested token spans.
  '.cm-lp-h1': { fontSize: '1.45em', fontWeight: '600', color: 'var(--color-shell-ink, #e7e4df) !important' },
  '.cm-lp-h2': { fontSize: '1.25em', fontWeight: '600', color: 'var(--color-shell-ink, #e7e4df) !important' },
  '.cm-lp-h3': { fontSize: '1.1em', fontWeight: '600', color: 'var(--color-shell-ink, #e7e4df) !important' },
  '.cm-lp-h4': { fontSize: '1em', fontWeight: '600', color: 'var(--color-shell-ink, #e7e4df) !important' },
  '.cm-lp-h5': { fontSize: '1em', fontWeight: '600', color: 'var(--color-shell-ink, #e7e4df) !important' },
  '.cm-lp-h6': { fontSize: '1em', fontWeight: '600', opacity: '0.8', color: 'var(--color-shell-ink, #e7e4df) !important' },
  // oneDark colors the heading TOKEN in a nested span — pull it back to ink.
  '.cm-lp-h1 span, .cm-lp-h2 span, .cm-lp-h3 span, .cm-lp-h4 span, .cm-lp-h5 span, .cm-lp-h6 span':
    { color: 'inherit !important' },
  '.cm-lp-strong': { fontWeight: '700', color: 'var(--color-shell-ink, #e7e4df) !important' },
  '.cm-lp-em': { fontStyle: 'italic' },
  '.cm-lp-strike': { textDecoration: 'line-through', opacity: '0.75' },
  '.cm-lp-code': {
    fontFamily: "'IBM Plex Mono', ui-monospace, monospace",
    fontSize: '0.9em',
    background: 'var(--color-surface-elevated, #1c1b22)',
    borderRadius: '3px',
    padding: '0.5px 4px',
  },
  '.cm-lp-link': { color: 'var(--color-primary, #e0653a)' },
  '.cm-lp-codeblock': {
    fontFamily: "'IBM Plex Mono', ui-monospace, monospace",
    fontSize: '12px',
    lineHeight: '1.5',
    // surface-elevated like the reading view's pre: surface-base would be
    // invisible on the editor's shell-panel background.
    background: 'var(--color-surface-elevated, #1c1b22)',
  },
  '.cm-lp-quote': {
    borderLeft: '2px solid var(--color-hairline, #211f26)',
    color: 'var(--color-muted, #928d99)',
    fontStyle: 'italic',
  },
  '.cm-lp-bullet': { color: 'var(--color-primary, #e0653a)' },
  '.cm-lp-frontmatter': {
    fontFamily: "'IBM Plex Mono', ui-monospace, monospace",
    fontSize: '0.85em',
    background: 'rgba(255, 255, 255, 0.03)',
  },
  // Revealed table source: monospace so the auto-formatter's pipe alignment
  // holds up visually, and NO wrapping — a wrapped aligned table is worse
  // than an unaligned one. Wide tables overflow horizontally; the scroller
  // follows the cursor. width: max-content stretches the line box (and its
  // background tint) over the full overflowing row — otherwise the bg stops
  // at the readable-column cap while the text keeps going.
  '.cm-lp-tablesrc': {
    fontFamily: "'IBM Plex Mono', ui-monospace, monospace",
    fontSize: '0.85em',
    background: 'rgba(255, 255, 255, 0.03)',
    whiteSpace: 'pre',
    width: 'max-content',
    minWidth: '100%',
  },
  '.cm-lp-table': { cursor: 'text', padding: '2px 0' },
  // Rendered callout widget: the fancy .callout markup (icon, colored title,
  // tinted body) comes from index.css; this spaces the block like the
  // surrounding prose lines. whiteSpace reset: .cm-content is pre-wrap, which
  // would turn the literal newlines between the rendered elements into blank
  // lines inside the callout.
  '.cm-lp-callout-block': { cursor: 'text', padding: '2px 0', whiteSpace: 'normal' },
  '.cm-lp-callout-block .callout': { margin: '2px 0' },
  // Frontmatter Properties card (shared .fm-card styles from index.css);
  // click drops into the raw source like the other block widgets.
  '.cm-lp-fm': { cursor: 'text', padding: '2px 0', whiteSpace: 'normal' },
  '.cm-lp-fm .fm-card': { margin: '2px 0' },
  // Rendered raw-HTML block (e.g. a README's centered `<p align="center">`
  // demo). whiteSpace reset for the same pre-wrap reason as callouts.
  '.cm-lp-html': { cursor: 'text', padding: '2px 0', whiteSpace: 'normal' },
  '.cm-lp-html img': { maxWidth: '100%', height: 'auto' },
  // Inline images (badges, `![](…)`) render at their natural size, capped to
  // the column; a broken/blank src just collapses rather than showing an icon.
  '.cm-lp-img': {
    display: 'inline-block',
    maxWidth: '100%',
    height: 'auto',
    verticalAlign: 'text-bottom',
    cursor: 'pointer',
  },
  // Rendered tables match the reading view's .prose table frame: hairline
  // cell borders, tinted header, rounded outer edge.
  '.cm-lp-table table': {
    borderCollapse: 'separate',
    borderSpacing: '0',
    margin: '2px 0',
    fontSize: '0.95em',
    border: '1px solid var(--color-hairline, #211f26)',
    borderRadius: 'var(--radius-md, 4px)',
    overflow: 'hidden',
  },
  '.cm-lp-table th, .cm-lp-table td': {
    border: '0',
    borderRight: '1px solid var(--color-hairline, #211f26)',
    borderBottom: '1px solid var(--color-hairline, #211f26)',
    padding: '3px 10px',
    textAlign: 'left',
  },
  '.cm-lp-table th:last-child, .cm-lp-table td:last-child': { borderRight: '0' },
  '.cm-lp-table tr:last-child td': { borderBottom: '0' },
  '.cm-lp-table th': {
    background: 'var(--color-surface-elevated, #1c1b22)',
    fontWeight: '600',
  },
});

export function livePreview(opts?: { maxLineWidth?: number; baseDir?: string }): Extension {
  const width = opts?.maxLineWidth ?? 0;
  return [
    // Scope the prose font to live-preview editors only.
    EditorView.editorAttributes.of({ class: 'cm-lp' }),
    // The document's directory, for resolving relative image srcs.
    baseDirFacet.of(opts?.baseDir ?? ''),
    // Prose wraps; horizontal scrolling is a source-mode behavior.
    EditorView.lineWrapping,
    // Readable line length (Obsidian-style): center a prose column instead
    // of running lines the full window width. The 24px side padding mirrors
    // the reading view's px-6 container, so Edit ↔ Preview keeps the text at
    // the same x even when the pane is narrower than the column. Inline
    // style so it is plainly inspectable (and testable) on .cm-content.
    ...(width > 0
      ? [
          EditorView.contentAttributes.of({
            style: `max-width:${width}px;margin:0 auto;padding-left:24px;padding-right:24px;box-sizing:content-box;`,
          }),
        ]
      : []),
    livePreviewPlugin,
    blockWidgetField,
    blockWidgetCursorEntry,
    tableAutoFormat,
    livePreviewTheme,
  ];
}
