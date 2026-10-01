/**
 * Line comments in one editor of the diff pane.
 *
 * The editor gets its own line-number gutter. A hover on a number shows a
 * `+` button over the right edge of the gutter, so the gutter keeps its width
 * and the number stays readable. A press on a number starts a range, a drag
 * over more numbers extends it, and the release opens the comment box under
 * the last line of the range. The drag and the open box tint the selected
 * rows and their numbers, removed rows inside the range included. The `+`
 * hides while a drag or a box is open. The stored comments of the file show as blocks under their last
 * line, each with a Resolve button and, where the pane has a chat, an Attach
 * button that puts the comment back in the composer.
 *
 * A drag over the text opens the same box. CodeMirror selects the text, and
 * the drag takes the whole lines of its two ends. The browser selection hides
 * during the drag, so only the tint shows the range. A drag that ends with no
 * comment leaves text selected, so the user can copy it: a click selects
 * nothing and opens no box, and Cancel selects the lines of the range. A
 * comment range is always whole lines, so no character position is kept.
 *
 * A removed row of the unified view has no line of its own in the editor. The
 * gutter numbers it with its line in the base text, as a patch does. A base
 * number does not take a comment, because the unified view comments on the
 * current side only.
 *
 * The editor owns only this display state. The comments come from the query
 * layer through `setComments`, and the host stores a new comment.
 */
import { Show, createSignal, onMount } from 'solid-js';
import { render } from 'solid-js/web';
import {
  StateEffect,
  StateField,
  type EditorState,
  type Extension,
  type Range,
} from '@codemirror/state';
import {
  Decoration,
  EditorView,
  GutterMarker,
  WidgetType,
  gutter,
  type DecorationSet,
} from '@codemirror/view';
import { getChunks, getOriginalDoc } from '@codemirror/merge';
import { type CommentSide, type DiffComment } from '@/lib/diffset';

/** The first and the last line of a range. Both are 1-based and inclusive. */
export interface LineSpan {
  first: number;
  last: number;
}

/** What the editor asks of its owner. */
export interface CommentHost {
  /** The side that the line numbers of this editor count on. */
  side: CommentSide;
  /**
   * Stores a comment, and attaches it to the chat of the pane. The promise
   * rejects with the reason of a refusal.
   */
  comment(span: LineSpan, text: string): Promise<void>;
  /** The chat that takes the comment, or null when the pane has none. */
  chat(): string | null;
  /** Marks a stored comment resolved. The promise rejects with the reason of a refusal. */
  resolve(commentId: string): Promise<void>;
  /** True while the comment has a chip in the composer of the chat of the pane. */
  attached(commentId: string): boolean;
  /** Puts the comment back into the composer of the chat of the pane. */
  attach(comment: DiffComment): void;
}

interface CommentUi {
  /** The line under the pointer. */
  hover: number | null;
  /** The rows of a drag that has not ended. See `dragSpan`. */
  drag: { anchor: number; head: number } | null;
  /** The range of the open comment box. */
  draft: LineSpan | null;
  /** The stored comments of this file on this side. */
  comments: readonly DiffComment[];
}

const setHover = StateEffect.define<number | null>();
const setDrag = StateEffect.define<{ anchor: number; head: number } | null>();
const setDraft = StateEffect.define<LineSpan | null>();
/** Gives the editor the stored comments of its file and side. */
export const setComments = StateEffect.define<readonly DiffComment[]>();

const uiField = StateField.define<CommentUi>({
  create: () => ({ hover: null, drag: null, draft: null, comments: [] }),
  update(value, tr) {
    let next = value;
    for (const effect of tr.effects) {
      if (effect.is(setHover)) next = { ...next, hover: effect.value };
      else if (effect.is(setDrag)) next = { ...next, drag: effect.value };
      else if (effect.is(setDraft)) next = { ...next, draft: effect.value };
      else if (effect.is(setComments)) next = { ...next, comments: effect.value };
    }
    return next;
  },
});

/**
 * The lines of a drag from the row `anchor` to the row `head`.
 *
 * A row is a line number, or `n - 0.5` for a removed row of the chunk above
 * line `n`. A comment takes whole lines of the current side, and the chunk is
 * in the range only when the lines on both sides of it are. So a removed row
 * at the top end of a drag takes the line above its chunk, and at the bottom
 * end the line under it.
 */
export function dragSpan(anchor: number, head: number): LineSpan {
  return {
    first: Math.max(1, Math.floor(Math.min(anchor, head))),
    last: Math.ceil(Math.max(anchor, head)),
  };
}

/** The lines that show as selected: the drag, or else the open box. */
function selected(ui: CommentUi): LineSpan | null {
  return ui.drag ? dragSpan(ui.drag.anchor, ui.drag.head) : ui.draft;
}

/** "Line 5" or "Lines 5-7". */
export function spanLabel(span: LineSpan): string {
  return span.last > span.first ? `Lines ${span.first}-${span.last}` : `Line ${span.first}`;
}

class LineMarker extends GutterMarker {
  constructor(
    readonly line: number,
    readonly hovered: boolean,
    readonly chosen: boolean,
  ) {
    super();
    // The tint of a selected number is on the whole gutter cell.
    this.elementClass = chosen ? 'cm-diff-selected-number' : '';
  }

  eq(other: LineMarker): boolean {
    return (
      other.line === this.line && other.hovered === this.hovered && other.chosen === this.chosen
    );
  }

  toDOM(): Node {
    const el = document.createElement('div');
    el.className = 'cm-diff-line';
    el.dataset.line = String(this.line);
    el.dataset.testid = `diff-line-${this.line}`;
    el.setAttribute('aria-selected', String(this.chosen));
    if (this.hovered) {
      const add = document.createElement('button');
      add.type = 'button';
      add.className = 'cm-diff-comment-add';
      add.textContent = '+';
      add.title = 'Comment on this line';
      add.setAttribute('aria-label', `Comment on line ${this.line}`);
      add.dataset.testid = 'diff-comment-add';
      el.appendChild(add);
    }
    el.appendChild(document.createTextNode(String(this.line)));
    return el;
  }
}

/**
 * The hidden element that gives the gutter its width: the widest number. It
 * names no line, so a search for the number of a line cannot find it.
 */
class SpacerMarker extends GutterMarker {
  constructor(readonly lines: number) {
    super();
  }

  eq(other: SpacerMarker): boolean {
    return other.lines === this.lines;
  }

  toDOM(): Node {
    const el = document.createElement('div');
    el.className = 'cm-diff-line';
    el.textContent = String(this.lines);
    return el;
  }
}

/**
 * The base line numbers of one removed chunk, beside its rows.
 *
 * Each number takes the height of its row. A wrapped row is taller than one
 * line, so the marker reads the heights of the rows when they change.
 */
class BaseLinesMarker extends GutterMarker {
  constructor(
    readonly first: number,
    readonly count: number,
    /** The element of the removed chunk, when the editor drew it. */
    readonly chunk: HTMLElement | null,
    readonly chosen: boolean,
  ) {
    super();
    this.elementClass = chosen ? 'cm-diff-selected-number' : '';
  }

  eq(other: BaseLinesMarker): boolean {
    return (
      other.first === this.first &&
      other.count === this.count &&
      other.chunk === this.chunk &&
      other.chosen === this.chosen
    );
  }

  toDOM(): Node {
    const el = document.createElement('div');
    el.className = 'cm-diff-base-lines';
    for (let i = 0; i < this.count; i++) {
      const row = el.appendChild(document.createElement('div'));
      row.className = 'cm-diff-line';
      row.dataset.testid = `diff-base-line-${this.first + i}`;
      row.textContent = String(this.first + i);
    }
    const chunk = this.chunk;
    if (chunk && typeof ResizeObserver !== 'undefined') {
      const fit = () => {
        chunk.querySelectorAll<HTMLElement>('.cm-deletedLine').forEach((line, i) => {
          const row = el.children[i] as HTMLElement | undefined;
          const height = line.getBoundingClientRect().height;
          if (row && height > 0) row.style.height = `${height}px`;
        });
      };
      const observer = new ResizeObserver(fit);
      observer.observe(chunk);
      observers.set(el, observer);
    }
    return el;
  }

  destroy(dom: Node): void {
    observers.get(dom as HTMLElement)?.disconnect();
    observers.delete(dom as HTMLElement);
  }
}

/** The height observer of each base-number marker, by its element. */
const observers = new WeakMap<HTMLElement, ResizeObserver>();

/**
 * The base numbers of the removed chunk that a block widget draws, or null
 * for any other widget. The chunk sits above a current line, so it is inside
 * a range that holds that line and the line above it. The removed chunk of `@codemirror/merge` sits at the
 * start of its chunk in the current text. Its widget builds its element on
 * demand, and the spacer of the split view does not, so `buildDOM` tells the
 * two apart.
 */
function baseLines(view: EditorView, widget: WidgetType, from: number): BaseLinesMarker | null {
  if (!('buildDOM' in widget)) return null;
  const chunks = getChunks(view.state);
  if (chunks?.side !== 'b') return null;
  const chunk = chunks.chunks.find((c) => c.fromB === from && c.fromA < c.toA);
  if (!chunk) return null;
  const base = getOriginalDoc(view.state);
  const first = base.lineAt(chunk.fromA).number;
  const count = base.sliceString(chunk.fromA, chunk.endA).split('\n').length;
  const dom = (widget as { dom?: HTMLElement | null }).dom ?? null;
  const span = selected(view.state.field(uiField));
  const below = view.state.doc.lineAt(from).number;
  const chosen = !!span && below > span.first && below <= span.last;
  return new BaseLinesMarker(first, count, dom, chosen);
}

/**
 * The line of the number under an event.
 *
 * The gutter gives each handler the line at the height of the element. The
 * element names its own line, so this does not depend on the layout.
 */
function lineOf(event: Event): number | null {
  const target = event.target as Element | null;
  const el = target?.closest?.('[data-line]') as HTMLElement | null;
  return el ? Number(el.dataset.line) : null;
}

/**
 * The row of the text under an event: a line, or a removed row as
 * `n - 0.5` (see `dragSpan`). A wrapped line is one element, so all of it is
 * one row. Null outside the rows of this editor, for example over a comment.
 */
function rowAt(view: EditorView, target: EventTarget | null): number | null {
  const el = (target as Element | null)?.closest?.('.cm-line, .cm-deletedChunk');
  if (!el || !view.contentDOM.contains(el)) return null;
  const line = view.state.doc.lineAt(view.posAtDOM(el)).number;
  return el.classList.contains('cm-deletedChunk') ? line - 0.5 : line;
}

/**
 * A press on the text. CodeMirror selects the text as the pointer moves, and
 * the drag follows the rows. A press while a box is open selects text only,
 * so that the text of the box is safe.
 */
function startTextDrag(view: EditorView, event: MouseEvent): boolean {
  const ui = view.state.field(uiField);
  const anchor = rowAt(view, event.target);
  if (event.button !== 0 || anchor === null || ui.drag || ui.draft) return false;
  const move = (e: MouseEvent) => {
    // A click selects nothing, so it is no drag.
    if (view.state.selection.main.empty) return;
    const drag = view.state.field(uiField).drag;
    // Over a line number, the gutter moves the head itself.
    const head = rowAt(view, e.target) ?? drag?.head ?? anchor;
    if (drag?.head !== head) view.dispatch({ effects: setDrag.of({ anchor, head }) });
  };
  const release = (e: MouseEvent) => {
    window.removeEventListener('mousemove', move);
    window.removeEventListener('mouseup', release);
    if (!view.dom.isConnected) return;
    move(e);
    endDrag(view);
  };
  // On the window, not the document: the listeners of CodeMirror on the
  // document then select first, and the test of the selection above is current.
  window.addEventListener('mousemove', move);
  window.addEventListener('mouseup', release);
  // CodeMirror must still take the press, to select the text.
  return false;
}

/** Ends a drag: the range becomes the range of the comment box. */
function endDrag(view: EditorView): void {
  const span = selected(view.state.field(uiField));
  if (!view.state.field(uiField).drag || !span) return;
  view.dispatch({ effects: [setDrag.of(null), setDraft.of(span)] });
}

const lineGutter = gutter({
  class: 'cm-lineNumbers cm-diff-lines',
  lineMarker(view, block) {
    const line = view.state.doc.lineAt(block.from).number;
    const ui = view.state.field(uiField);
    const span = selected(ui);
    const chosen = !!span && line >= span.first && line <= span.last;
    const busy = ui.drag !== null || ui.draft !== null;
    return new LineMarker(line, !busy && ui.hover === line, chosen);
  },
  lineMarkerChange: (update) => update.startState.field(uiField) !== update.state.field(uiField),
  widgetMarker: (view, widget, block) => baseLines(view, widget, block.from),
  initialSpacer: (view) => new SpacerMarker(view.state.doc.lines),
  domEventHandlers: {
    mousedown(view, _block, event) {
      const line = lineOf(event);
      if (line === null || (event as MouseEvent).button !== 0) return false;
      view.dispatch({ effects: setDrag.of({ anchor: line, head: line }) });
      const release = () => {
        document.removeEventListener('mouseup', release);
        // The editor can go away before the release, for example on a refresh.
        if (view.dom.isConnected) endDrag(view);
      };
      document.addEventListener('mouseup', release);
      return true;
    },
    mouseover(view, _block, event) {
      const line = lineOf(event);
      const ui = view.state.field(uiField);
      const effects: StateEffect<unknown>[] = [];
      if (line !== ui.hover) effects.push(setHover.of(line));
      if (ui.drag && line !== null && (event as MouseEvent).buttons & 1 && line !== ui.drag.head) {
        effects.push(setDrag.of({ anchor: ui.drag.anchor, head: line }));
      }
      if (effects.length > 0) view.dispatch({ effects });
      return false;
    },
    mouseleave(view) {
      if (view.state.field(uiField).hover !== null) view.dispatch({ effects: setHover.of(null) });
      return false;
    },
  },
});

/**
 * One stored comment, under the last line of its range.
 *
 * **Resolve** settles the comment. **Attach** puts it back into the composer
 * of the chat of the pane, which is the other half of the pair: the `×` of a
 * chip deletes the comment. The body is a Solid root, because the chip of a
 * comment can come and go while the widget stays.
 */
class CommentWidget extends WidgetType {
  constructor(
    readonly comment: DiffComment,
    readonly host: CommentHost,
  ) {
    super();
  }

  eq(other: CommentWidget): boolean {
    const a = this.comment;
    const b = other.comment;
    return (
      a.id === b.id &&
      a.body === b.body &&
      a.line_range.start === b.line_range.start &&
      a.line_range.end === b.line_range.end &&
      this.host === other.host
    );
  }

  toDOM(): HTMLElement {
    const block = commentBlock();
    const card = block.appendChild(document.createElement('div'));
    card.className = 'cm-diff-comment';
    card.dataset.testid = 'diff-comment';
    disposers.set(
      block,
      render(() => <StoredComment comment={this.comment} host={this.host} />, card),
    );
    return block;
  }

  destroy(dom: HTMLElement): void {
    disposers.get(dom)?.();
    disposers.delete(dom);
  }

  ignoreEvent(): boolean {
    return true;
  }
}

/**
 * The block of one comment widget: the gap around the card, as padding.
 *
 * The gap must NOT be a margin. CodeMirror measures a block widget with
 * `getBoundingClientRect`, which leaves a margin out, so a margin here makes
 * the height map short. The rows then flow lower than the gutter believes,
 * and every change bar below the widget rides high by that margin. The
 * browser test `e2e/diff-change-bar.spec.ts` fails when a margin comes back.
 */
function commentBlock(): HTMLElement {
  const el = document.createElement('div');
  el.className = 'cm-diff-comment-block';
  return el;
}

/** The head and the body of one stored comment. */
function StoredComment(props: { comment: DiffComment; host: CommentHost }) {
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const who = () =>
    `${props.comment.author === 'agent' ? 'Agent' : 'You'} · ${spanLabel({
      first: props.comment.line_range.start,
      last: props.comment.line_range.end - 1,
    })}`;
  const resolve = () => {
    setBusy(true);
    setError(null);
    // A success removes the comment from the list, and this widget with it.
    props.host.resolve(props.comment.id).catch((err: unknown) => {
      setError(err instanceof Error ? err.message : 'The comment was not resolved');
      setBusy(false);
    });
  };
  return (
    <>
      <div class="cm-diff-comment-head">
        <span>{who()}</span>
        <span class="cm-diff-comment-actions">
          <Show when={props.host.chat() !== null}>
            <Show
              when={!props.host.attached(props.comment.id)}
              fallback={
                <span class="cm-diff-comment-state" data-testid="diff-comment-attached">
                  In the composer
                </span>
              }
            >
              <button
                type="button"
                class="cm-diff-comment-action"
                data-testid="diff-comment-attach"
                title="Put this comment in the composer of this chat again"
                onClick={() => props.host.attach(props.comment)}
              >
                Attach
              </button>
            </Show>
          </Show>
          <button
            type="button"
            class="cm-diff-comment-action"
            data-testid="diff-comment-resolve"
            title="Mark this comment resolved. It leaves the open comments."
            disabled={busy()}
            onClick={resolve}
          >
            Resolve
          </button>
        </span>
      </div>
      <div class="cm-diff-comment-body">{props.comment.body}</div>
      <Show when={error()}>
        {(message) => <div class="cm-diff-comment-error">{message()}</div>}
      </Show>
    </>
  );
}

/** The disposal of the Solid root of each open box, by its element. */
const disposers = new WeakMap<HTMLElement, () => void>();

/** The comment box, under the last line of the range. */
class BoxWidget extends WidgetType {
  constructor(
    readonly span: LineSpan,
    readonly host: CommentHost,
  ) {
    super();
  }

  eq(other: BoxWidget): boolean {
    return (
      other.span.first === this.span.first &&
      other.span.last === this.span.last &&
      other.host === this.host
    );
  }

  toDOM(view: EditorView): HTMLElement {
    const block = commentBlock();
    const card = block.appendChild(document.createElement('div'));
    card.className = 'cm-diff-comment-box';
    const close = () => view.dispatch({ effects: setDraft.of(null) });
    // The text field takes the focus after the mount, and the browser then
    // loses the selection of a text drag. Cancel selects the whole lines of
    // the range, so that the user can copy them. A gutter drag selects no
    // text, so its Cancel selects nothing.
    const fromText = !view.state.selection.main.empty;
    const cancel = () => {
      close();
      if (!fromText) return;
      const doc = view.state.doc;
      const from = view.domAtPos(doc.line(this.span.first).from);
      const to = view.domAtPos(doc.line(Math.min(doc.lines, this.span.last)).to);
      document.getSelection()?.setBaseAndExtent(from.node, from.offset, to.node, to.offset);
    };
    const dispose = render(
      () => (
        <CommentBox
          label={spanLabel(this.span)}
          onComment={async (text) => {
            await this.host.comment(this.span, text);
            close();
          }}
          hasChat={this.host.chat() !== null}
          onCancel={cancel}
        />
      ),
      card,
    );
    disposers.set(block, dispose);
    return block;
  }

  destroy(dom: HTMLElement): void {
    disposers.get(dom)?.();
    disposers.delete(dom);
  }

  // The box owns its keys and clicks. The editor must not take them.
  ignoreEvent(): boolean {
    return true;
  }
}

const selectedLine = Decoration.line({ class: 'cm-diff-selected' });

function decorations(state: EditorState, host: CommentHost): DecorationSet {
  const ui = state.field(uiField);
  const lines = state.doc.lines;
  const ranges: Range<Decoration>[] = [];
  const span = selected(ui);
  if (span) {
    for (let n = Math.max(1, span.first); n <= Math.min(lines, span.last); n++) {
      ranges.push(selectedLine.range(state.doc.line(n).from));
    }
  }
  for (const comment of ui.comments) {
    const last = Math.min(lines, Math.max(1, comment.line_range.end - 1));
    const at = state.doc.line(last).to;
    ranges.push(
      Decoration.widget({ widget: new CommentWidget(comment, host), block: true, side: 1 }).range(
        at,
      ),
    );
  }
  if (ui.draft) {
    const at = state.doc.line(Math.min(lines, ui.draft.last)).to;
    ranges.push(
      Decoration.widget({ widget: new BoxWidget(ui.draft, host), block: true, side: 2 }).range(at),
    );
  }
  return Decoration.set(ranges, true);
}

/**
 * A tint of the info blue, for the selected range. The hue is far from the
 * red of a removed row and the green of an added row, so a selection does
 * not read as a change. The primary colour is a rust orange, too near red.
 * The token is an `r, g, b` triple with a light and a dark value.
 */
const tint = (alpha: number) => `rgba(var(--cru-color-callout-info), ${alpha})`;

const commentTheme = EditorView.theme({
  // During a drag, only the tint shows the range. The selection is on whole
  // lines, and the text of the browser selection is not.
  '&.cm-diff-dragging .cm-content ::selection, &.cm-diff-dragging .cm-content::selection': {
    background: 'transparent',
  },
  '.cm-diff-lines .cm-gutterElement': { cursor: 'pointer', userSelect: 'none' },
  // The `+` hangs out of the gutter, so the gutter must not clip it.
  '.cm-gutter.cm-diff-lines': { overflow: 'visible' },
  '.cm-diff-line': { position: 'relative' },
  '.cm-diff-base-lines': { cursor: 'default' },
  // The selected range. The numbers carry the strong tint and a bar. The
  // rows carry a light tint over their own, so a changed row keeps its tint.
  '.cm-gutterElement.cm-diff-selected-number': {
    backgroundColor: tint(0.28),
    boxShadow: `inset 2px 0 ${tint(1)}`,
    color: 'var(--color-shell-ink)',
  },
  '.cm-diff-line[aria-selected="true"]': { color: 'var(--color-shell-ink)', fontWeight: '600' },
  // Over the right padding of the number, the change bar and the left padding
  // of the row: 3 + 3 + 6 px. No digit and no text is under it.
  '.cm-diff-comment-add': {
    position: 'absolute',
    zIndex: '1',
    left: '100%',
    top: '0',
    width: '12px',
    padding: '0',
    border: 'none',
    lineHeight: 'inherit',
    borderRadius: 'var(--cru-radius-sm)',
    background: 'var(--color-primary)',
    color: 'var(--color-on-primary)',
    textAlign: 'center',
  },
  '.cm-line.cm-diff-selected': { backgroundImage: `linear-gradient(${tint(0.14)}, ${tint(0.14)})` },
  // A removed chunk between two selected lines is inside the range.
  '.cm-line.cm-diff-selected + .cm-deletedChunk:has(+ .cm-line.cm-diff-selected)': {
    backgroundImage: `linear-gradient(${tint(0.14)}, ${tint(0.14)})`,
  },
  // The gap is padding on the block, never a margin on the card: see
  // `commentBlock`.
  '.cm-diff-comment-block': { padding: '4px 8px 4px 0' },
  // The selected lines carry the anchor color; comments need no outer frame.
  '.cm-diff-comment, .cm-diff-comment-box': {
    padding: '4px 8px',
    border: 'none',
    background: 'transparent',
    fontFamily: 'var(--font-sans, sans-serif)',
    whiteSpace: 'pre-wrap',
  },
  '.cm-diff-comment-head': {
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'space-between',
    gap: '8px',
    color: 'var(--color-muted-dark)',
    marginBottom: '2px',
  },
  '.cm-diff-comment-actions': { display: 'flex', alignItems: 'center', gap: '6px' },
  '.cm-diff-comment-action': {
    padding: '0 6px',
    border: 'none',
    borderRadius: 'var(--cru-radius-sm)',
    background: 'transparent',
    color: 'var(--color-muted)',
    font: 'inherit',
    fontSize: 'var(--cru-font-floor)',
    cursor: 'pointer',
  },
  '.cm-diff-comment-action:hover': {
    background: 'var(--color-hover-wash)',
    color: 'var(--color-shell-ink)',
  },
  '.cm-diff-comment-action:focus-visible': {
    outline: '1px solid var(--color-focus-ring)',
    outlineOffset: '1px',
  },
  '.cm-diff-comment-action:disabled': { opacity: '0.5', cursor: 'default' },
  // The comment already has a chip. This is a state, not a control.
  '.cm-diff-comment-state': { fontSize: 'var(--cru-font-floor)' },
  '.cm-diff-comment-error': { color: 'var(--color-error)' },
  '.cm-diff-comment-error:empty': { display: 'none' },
});

/** The extensions of one diff editor that takes line comments. */
export function commentExtensions(host: CommentHost): Extension[] {
  return [
    uiField,
    lineGutter,
    EditorView.domEventHandlers({ mousedown: (event, view) => startTextDrag(view, event) }),
    EditorView.editorAttributes.compute([uiField], (state) => ({
      class: state.field(uiField).drag ? 'cm-diff-dragging' : '',
    })),
    EditorView.decorations.compute([uiField], (state) => decorations(state, host)),
    commentTheme,
  ];
}

const boxButton = 'rounded px-2 py-0.5 text-floor disabled:opacity-50 focus-ring';
/** Comment: the primary action of the box. */
const primaryButton = `${boxButton} border-primary bg-primary text-on-primary hover:bg-primary-hover hover:border-primary-hover disabled:hover:bg-primary disabled:hover:border-primary`;
/** Cancel. */
const secondaryButton = `${boxButton} border-hairline text-muted-dark hover:text-shell-ink hover:bg-hover-wash`;

/**
 * The box: a text field, then Comment and Cancel.
 *
 * **Comment** stores the comment and attaches it to the chat of the pane.
 * A pane with no chat still stores it, and the box says that no chat gets it.
 */
export function CommentBox(props: {
  label: string;
  onComment: (text: string) => Promise<void>;
  /** The pane has a chat that takes the comment. */
  hasChat: boolean;
  onCancel: () => void;
}) {
  const [text, setText] = createSignal('');
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  let input!: HTMLTextAreaElement;
  onMount(() => queueMicrotask(() => input.focus()));

  const comment = async () => {
    const value = text().trim();
    if (!value || busy()) return;
    setBusy(true);
    setError(null);
    try {
      await props.onComment(value);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'The comment was not saved');
      setBusy(false);
    }
  };

  return (
    <div data-testid="diff-comment-box" class="flex flex-col gap-1.5">
      <textarea
        ref={input}
        data-testid="diff-comment-input"
        rows={2}
        placeholder="Write a comment. Ctrl+Enter saves it."
        value={text()}
        onInput={(e) => setText(e.currentTarget.value)}
        onKeyDown={(e) => {
          if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
            e.preventDefault();
            void comment();
          } else if (e.key === 'Escape') {
            e.preventDefault();
            props.onCancel();
          }
        }}
        class="w-full resize-y rounded-lg border-0 bg-control px-3 py-2 text-xs text-shell-ink focus-ring"
      />
      <Show when={error()}>
        {(message) => <span class="text-floor text-error">{message()}</span>}
      </Show>
      <Show when={!props.hasChat}>
        <span class="text-floor text-muted-dark" data-testid="diff-comment-no-chat">
          No chat takes this comment. The diff pane stores it.
        </span>
      </Show>
      {/* The label shares the row of the buttons, so the box takes one row less. */}
      <div class="flex items-center gap-1.5">
        <span class="mr-auto text-floor text-muted-dark">{props.label}</span>
        <button
          type="button"
          data-testid="diff-comment-cancel"
          onClick={() => props.onCancel()}
          class={secondaryButton}
        >
          Cancel
        </button>
        <button
          type="button"
          data-testid="diff-comment-submit"
          disabled={busy() || !text().trim()}
          onClick={() => void comment()}
          class={primaryButton}
        >
          Comment
        </button>
      </div>
    </div>
  );
}
