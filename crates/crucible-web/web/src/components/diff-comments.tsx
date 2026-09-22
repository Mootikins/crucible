/**
 * Line comments in one editor of the diff pane.
 *
 * The editor gets its own line-number gutter. A hover on a number shows a
 * `+` button. A press on a number starts a range, a drag over more numbers
 * extends it, and the release opens the comment box under the last line of
 * the range. The drag and the open box tint the selected rows and their
 * numbers. The stored comments of the file show as blocks under their last
 * line, each with a Resolve button.
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
}

interface CommentUi {
  /** The line under the pointer. */
  hover: number | null;
  /** The range of a drag that has not ended. */
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

/** The lines that show as selected: the drag, or else the open box. */
function selected(ui: CommentUi): LineSpan | null {
  if (ui.drag) {
    const { anchor, head } = ui.drag;
    return { first: Math.min(anchor, head), last: Math.max(anchor, head) };
  }
  return ui.draft;
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
  ) {
    super();
  }

  eq(other: BaseLinesMarker): boolean {
    return other.first === this.first && other.count === this.count && other.chunk === this.chunk;
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
 * for any other widget. The removed chunk of `@codemirror/merge` sits at the
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
  return new BaseLinesMarker(first, count, dom);
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
    return new LineMarker(line, ui.hover === line, chosen);
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

/** One stored comment, under the last line of its range, with its Resolve button. */
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
    const { comment } = this;
    const el = document.createElement('div');
    el.className = 'cm-diff-comment';
    el.dataset.testid = 'diff-comment';
    const head = document.createElement('div');
    head.className = 'cm-diff-comment-head';
    const who = document.createElement('span');
    who.textContent = `${comment.author === 'agent' ? 'Agent' : 'You'} · ${spanLabel({
      first: comment.line_range.start,
      last: comment.line_range.end - 1,
    })}`;
    const resolve = document.createElement('button');
    resolve.type = 'button';
    resolve.className = 'cm-diff-comment-resolve';
    resolve.dataset.testid = 'diff-comment-resolve';
    resolve.textContent = 'Resolve';
    resolve.title = 'Mark this comment resolved. It leaves the open comments.';
    const error = document.createElement('div');
    error.className = 'cm-diff-comment-error';
    resolve.addEventListener('click', () => {
      resolve.disabled = true;
      error.textContent = '';
      // A success removes the comment from the list, and this widget with it.
      this.host.resolve(comment.id).catch((err: unknown) => {
        error.textContent = err instanceof Error ? err.message : 'The comment was not resolved';
        resolve.disabled = false;
      });
    });
    head.append(who, resolve);
    const body = document.createElement('div');
    body.className = 'cm-diff-comment-body';
    body.textContent = comment.body;
    el.append(head, body, error);
    return el;
  }

  ignoreEvent(): boolean {
    return true;
  }
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
    const el = document.createElement('div');
    el.className = 'cm-diff-comment-box';
    const close = () => view.dispatch({ effects: setDraft.of(null) });
    const dispose = render(
      () => (
        <CommentBox
          label={spanLabel(this.span)}
          onComment={async (text) => {
            await this.host.comment(this.span, text);
            close();
          }}
          hasChat={this.host.chat() !== null}
          onCancel={close}
        />
      ),
      el,
    );
    disposers.set(el, dispose);
    return el;
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
  '.cm-diff-lines .cm-gutterElement': { cursor: 'pointer', userSelect: 'none' },
  '.cm-diff-line': { position: 'relative', paddingLeft: '1.25em' },
  '.cm-diff-base-lines': { cursor: 'default' },
  // The selected range. The numbers carry the strong tint and a bar. The
  // rows carry a light tint over their own, so a changed row keeps its tint.
  '.cm-gutterElement.cm-diff-selected-number': {
    backgroundColor: tint(0.28),
    boxShadow: `inset 2px 0 ${tint(1)}`,
    color: 'var(--color-shell-ink)',
  },
  '.cm-diff-line[aria-selected="true"]': { color: 'var(--color-shell-ink)', fontWeight: '600' },
  '.cm-diff-comment-add': {
    position: 'absolute',
    left: '0',
    top: '0',
    width: '1.1em',
    lineHeight: 'inherit',
    borderRadius: 'var(--cru-radius-sm)',
    background: 'var(--color-primary)',
    color: 'var(--color-on-primary)',
    textAlign: 'center',
  },
  '.cm-line.cm-diff-selected': { backgroundImage: `linear-gradient(${tint(0.14)}, ${tint(0.14)})` },
  '.cm-diff-comment, .cm-diff-comment-box': {
    margin: '4px 8px 4px 0',
    padding: '6px 8px',
    border: '1px solid var(--color-hairline)',
    borderRadius: 'var(--cru-radius-md)',
    background: 'var(--color-shell-bg)',
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
  '.cm-diff-comment-resolve': {
    padding: '0 6px',
    border: '1px solid var(--color-hairline)',
    borderRadius: 'var(--cru-radius-sm)',
    background: 'transparent',
    color: 'var(--color-muted)',
    font: 'inherit',
    fontSize: 'var(--cru-font-floor)',
    cursor: 'pointer',
  },
  '.cm-diff-comment-resolve:hover': {
    background: 'var(--color-hover-wash)',
    color: 'var(--color-shell-ink)',
  },
  '.cm-diff-comment-resolve:focus-visible': {
    outline: '1px solid var(--color-focus-ring)',
    outlineOffset: '1px',
  },
  '.cm-diff-comment-resolve:disabled': { opacity: '0.5', cursor: 'default' },
  '.cm-diff-comment-error': { color: 'var(--color-error)' },
  '.cm-diff-comment-error:empty': { display: 'none' },
});

/** The extensions of one diff editor that takes line comments. */
export function commentExtensions(host: CommentHost): Extension[] {
  return [
    uiField,
    lineGutter,
    EditorView.decorations.compute([uiField], (state) => decorations(state, host)),
    commentTheme,
  ];
}

const boxButton = 'rounded border px-2 py-0.5 text-floor disabled:opacity-50 focus-ring';
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
function CommentBox(props: {
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
      <span class="text-floor text-muted-dark">{props.label}</span>
      <textarea
        ref={input}
        data-testid="diff-comment-input"
        rows={3}
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
        class="w-full resize-y rounded border border-hairline bg-transparent px-2 py-1 text-xs text-shell-ink"
      />
      <Show when={error()}>
        {(message) => <span class="text-floor text-error">{message()}</span>}
      </Show>
      <Show when={!props.hasChat}>
        <span class="text-floor text-muted-dark" data-testid="diff-comment-no-chat">
          No chat takes this comment. The diff pane stores it.
        </span>
      </Show>
      <div class="flex items-center gap-1.5">
        <button
          type="button"
          data-testid="diff-comment-submit"
          disabled={busy() || !text().trim()}
          onClick={() => void comment()}
          class={primaryButton}
        >
          Comment
        </button>
        <button
          type="button"
          data-testid="diff-comment-cancel"
          onClick={() => props.onCancel()}
          class={secondaryButton}
        >
          Cancel
        </button>
      </div>
    </div>
  );
}
