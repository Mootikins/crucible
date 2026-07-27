import {
  Component,
  For,
  Show,
  createEffect,
  createMemo,
  createSignal,
  onCleanup,
  onMount,
} from 'solid-js';
import { PanelShell } from '../PanelShell';
import { CanvasNodeView } from './CanvasNodeView';
import { NotePicker } from './NotePicker';
import {
  CanvasCardChrome,
  SwatchRow,
  ToolbarButton,
  ToolbarShell,
  type EdgeSide,
  type ResizeCorner,
} from './CanvasCardChrome';
import { ArrowLeft, ArrowRight, Palette, Pencil, Trash2 } from '@/lib/icons';
import { getCanvas, saveCanvas, rawFileUrl } from '@/lib/api';
import { notificationActions } from '@/stores/notificationStore';
import { openNoteInEditor } from '@/lib/note-actions';
import {
  anchorPoint,
  boundsOf,
  edgePath,
  fromEndOf,
  inferSide,
  resolveCanvasColor,
  toEndOf,
  type CanvasDoc,
  type CanvasEdge,
  type CanvasNode,
  type GroupNode,
  type CanvasResponse,
  type RejectedRef,
} from '@/lib/canvas-types';
import {
  frameRect,
  isLowDetail,
  panBy,
  screenToCanvas,
  snap,
  visibleNodes,
  zoomAt,
  type Viewport,
} from '@/lib/canvas-viewport';
import {
  addEdge,
  addNode,
  updateEdge,
  canRedo,
  canUndo,
  commit,
  duplicateNodes,
  groupSelection,
  initHistory,
  moveSelection,
  nodesInRect,
  redo,
  removeEdges,
  removeNodes,
  resizeNode,
  updateNode,
  undo,
  type History,
} from '@/lib/canvas-doc';

/**
 * The canvas editor.
 *
 * Nodes are DOM inside a transformed layer and edges are one SVG overlay — the
 * same shape Obsidian uses, and the only shape that lets a note card host a
 * live editor. Two things Obsidian gets partly wrong are built in from the
 * start: a generous overscan margin so panning does not remount cards at the
 * viewport edge, and level-of-detail applied to *every* node type including
 * media, which is precisely the exemption that makes zoomed-out Obsidian
 * canvases crawl.
 *
 * Document mutation and undo live in `lib/canvas-doc`, and viewport maths in
 * `lib/canvas-viewport`, so the fiddly rules are unit-testable without a DOM.
 */

interface CanvasPanelProps {
  /** Absolute path to the `.canvas` file. */
  filePath?: string;
}

type Drag =
  | { kind: 'pan'; startX: number; startY: number }
  | { kind: 'move'; startX: number; startY: number; origin: CanvasDoc }
  | { kind: 'marquee'; startX: number; startY: number; currentX: number; currentY: number }
  | {
      kind: 'resize';
      id: string;
      corner: ResizeCorner;
      startX: number;
      startY: number;
      origin: CanvasDoc;
    }
  | { kind: 'connect'; from: string; fromSide: EdgeSide; currentX: number; currentY: number };

const uid = (prefix: string) =>
  `${prefix}-${Math.random().toString(36).slice(2, 10)}${Date.now().toString(36)}`;

export const CanvasPanel: Component<CanvasPanelProps> = (props) => {
  const [history, setHistory] = createSignal<History>(initHistory({ nodes: [], edges: [] }));
  const [rejected, setRejected] = createSignal<RejectedRef[]>([]);
  const [kiln, setKiln] = createSignal('');
  const [error, setError] = createSignal<string | null>(null);
  const [loading, setLoading] = createSignal(false);
  const [dirty, setDirty] = createSignal(false);

  const [viewport, setViewport] = createSignal<Viewport>({
    x: 0,
    y: 0,
    zoom: 1,
    width: 1200,
    height: 800,
  });
  const [selected, setSelected] = createSignal<Set<string>>(new Set());
  // Selection and editing are separate states. A single click focuses a card so
  // it can be scrolled and styled; only a double click inside it opens the
  // editor, so ordinary selection never steals the keyboard.
  const [editingId, setEditingId] = createSignal<string | null>(null);
  // Edges are selectable in their own right — an arrow is a first-class object
  // with a colour, a label and two ends, and there was previously no way to
  // touch any of them once drawn.
  const [selectedEdge, setSelectedEdge] = createSignal<string | null>(null);
  const [pickingNote, setPickingNote] = createSignal(false);
  const [drag, setDrag] = createSignal<Drag | null>(null);
  const [snapping, setSnapping] = createSignal(true);

  let surface: HTMLDivElement | undefined;

  /// Pointer capture is best-effort: it throws for a pointer id the element
  /// does not own, and is absent entirely in jsdom. Losing capture degrades the
  /// drag slightly; letting it throw would abort the whole interaction.
  const capturePointer = (id: number) => {
    try {
      surface?.setPointerCapture(id);
    } catch {
      /* capture unavailable — the drag still tracks via the surface handlers */
    }
  };

  /// Capture is DEFERRED until the pointer has actually moved.
  ///
  /// Capturing on pointerdown retargets the subsequent `click` and `dblclick`
  /// to the capturing element, so a double click on a card was delivered to the
  /// surface instead — which read it as a click on empty space and created a
  /// new card rather than editing the one under the cursor. Capturing only once
  /// a drag is genuinely under way leaves click semantics intact, and has the
  /// side benefit that a twitchy click no longer starts a drag at all.
  const DRAG_THRESHOLD = 3;
  let pendingCapture: number | null = null;

  const armCapture = (id: number) => {
    pendingCapture = id;
  };

  const captureIfMoved = (e: PointerEvent, startX: number, startY: number) => {
    if (pendingCapture === null) return;
    if (Math.hypot(e.clientX - startX, e.clientY - startY) < DRAG_THRESHOLD) return;
    capturePointer(pendingCapture);
    pendingCapture = null;
  };

  const doc = () => history().present;
  const rejectedFor = (id: string) => rejected().find((r) => r.nodeId === id)?.reason;

  // --- loading / saving -----------------------------------------------------

  createEffect(() => {
    const path = props.filePath;
    if (!path) return;
    setLoading(true);
    setError(null);
    getCanvas(path)
      .then((res: CanvasResponse) => {
        // Kiln first. Node cards resolve their file against it, so setting the
        // document before it is known makes every card fetch a path missing its
        // base — one spurious 404 per card on every open.
        setKiln(res.kiln);
        setRejected(res.rejected);
        setHistory(initHistory(res.canvas));
        setDirty(false);
        queueMicrotask(frameAll);
      })
      .catch((e: unknown) => setError(e instanceof Error ? e.message : String(e)))
      .finally(() => setLoading(false));
  });

  let saveTimer: ReturnType<typeof setTimeout> | undefined;
  const scheduleSave = () => {
    setDirty(true);
    clearTimeout(saveTimer);
    saveTimer = setTimeout(save, 600);
  };
  onCleanup(() => {
    clearTimeout(saveTimer);
    // The panel is disposed and remounted on every tab switch, so cancelling
    // without flushing loses any edit made inside the debounce window — move a
    // card, switch tab, and it is gone with no warning.
    if (dirty()) void save();
  });

  const save = async () => {
    const path = props.filePath;
    if (!path) return;
    try {
      await saveCanvas(path, doc());
      setDirty(false);
    } catch (e) {
      // A refused save is nearly always containment: a node was pointed
      // outside the kiln. Surfacing it is the whole point of the advisory
      // layer — the write was already refused server-side.
      notificationActions.addNotification(
        'error',
        `Canvas not saved: ${e instanceof Error ? e.message : String(e)}`,
      );
    }
  };

  /** Apply a document mutation, push it on the undo stack, and schedule a save. */
  const mutate = (next: CanvasDoc) => {
    setHistory((h) => commit(h, next));
    scheduleSave();
  };

  // --- viewport -------------------------------------------------------------

  const measure = () => {
    if (!surface) return;
    const rect = surface.getBoundingClientRect();
    setViewport((v) => ({ ...v, width: rect.width, height: rect.height }));
  };

  const frameAll = () => {
    const bounds = boundsOf(doc().nodes);
    // frameRect declines when the viewport has no usable area, so an
    // unmeasured panel keeps its default zoom rather than snapping to minimum.
    if (bounds) setViewport((v) => frameRect(v, bounds));
  };

  const frameSelection = () => {
    const chosen = doc().nodes.filter((n) => selected().has(n.id));
    const bounds = boundsOf(chosen.length > 0 ? chosen : doc().nodes);
    if (bounds) setViewport((v) => frameRect(v, bounds));
  };

  onMount(() => {
    measure();
    const observer = new ResizeObserver(measure);
    if (surface) observer.observe(surface);
    onCleanup(() => observer.disconnect());
  });

  const onWheel = (e: WheelEvent) => {
    if (!surface) return;

    // Over a focused card, the wheel belongs to that card's content — the
    // point of focusing one is to read past its bottom edge without zooming
    // the whole canvas.
    // Only a SELECTED card takes the wheel. Without that check, merely passing
    // the pointer over a long card while zooming scrolled the card instead of
    // the canvas — the wheel belongs to the canvas until the user has said
    // which card they are working in.
    const card = (e.target as HTMLElement | null)?.closest?.('[data-canvas-scroll]');
    const owner = card?.closest?.('[data-node-id]')?.getAttribute('data-node-id');
    if (card && owner && selected().has(owner)) {
      // Walk up from the pointer to the card looking for whatever actually
      // scrolls — the markdown preview and CodeMirror each bring their own
      // scroll container, so the card wrapper itself never overflows.
      let el: HTMLElement | null = e.target as HTMLElement | null;
      while (el && el !== (card as HTMLElement).parentElement) {
        if (el.scrollHeight > el.clientHeight + 1) {
          const atTop = el.scrollTop <= 0 && e.deltaY < 0;
          const atBottom =
            el.scrollTop + el.clientHeight >= el.scrollHeight - 1 && e.deltaY > 0;
          if (!atTop && !atBottom) return;
          break;
        }
        el = el.parentElement;
      }
    }

    const rect = surface.getBoundingClientRect();
    if (e.ctrlKey || e.metaKey) {
      e.preventDefault();
      setViewport((v) => zoomAt(v, e.clientX - rect.left, e.clientY - rect.top, Math.exp(-e.deltaY / 400)));
      return;
    }
    e.preventDefault();
    // Shift swaps the axis, matching Obsidian and every other canvas tool.
    const [dx, dy] = e.shiftKey ? [-e.deltaY, 0] : [-e.deltaX, -e.deltaY];
    setViewport((v) => panBy(v, dx, dy));
  };

  // --- pointer interaction --------------------------------------------------

  const pointerCanvas = (e: PointerEvent) => {
    const rect = surface!.getBoundingClientRect();
    return screenToCanvas(viewport(), e.clientX - rect.left, e.clientY - rect.top);
  };

  const onSurfacePointerDown = (e: PointerEvent) => {
    if (!surface) return;
    // The shortcuts live on this element now, so it has to actually take focus
    // — otherwise they only work once the browser has incidentally focused it.
    surface.focus();
    armCapture(e.pointerId);
    const point = pointerCanvas(e);

    // Middle button or space-drag pans; anything else on empty space marquees.
    if (e.button === 1 || spaceHeld()) {
      setDrag({ kind: 'pan', startX: e.clientX, startY: e.clientY });
      return;
    }

    if (!e.shiftKey) setSelected(new Set<string>());
    setEditingId(null);
    setSelectedEdge(null);
    setDrag({ kind: 'marquee', startX: point.x, startY: point.y, currentX: point.x, currentY: point.y });
  };

  const onNodePointerDown = (e: PointerEvent, id: string) => {
    e.stopPropagation();
    if (!surface) return;
    surface.focus();
    armCapture(e.pointerId);

    setSelected((prev) => {
      const next = new Set<string>(e.shiftKey ? prev : []);
      if (e.shiftKey && prev.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
    // Clicking a different card leaves whatever was being edited.
    if (editingId() !== id) setEditingId(null);
    setSelectedEdge(null);

    // Alt-drag duplicates rather than moves, matching Obsidian.
    if (e.altKey) {
      const { doc: next, created } = duplicateNodes(doc(), new Set([id]), (i) => uid(`node${i}`));
      mutate(next);
      setSelected(new Set<string>(created));
    }

    setDrag({ kind: 'move', startX: e.clientX, startY: e.clientY, origin: doc() });
  };

  const onResizePointerDown = (e: PointerEvent, id: string, corner: ResizeCorner) => {
    e.stopPropagation();
    armCapture(e.pointerId);
    setDrag({ kind: 'resize', id, corner, startX: e.clientX, startY: e.clientY, origin: doc() });
  };

  const onConnectPointerDown = (e: PointerEvent, id: string, fromSide: EdgeSide) => {
    e.stopPropagation();
    armCapture(e.pointerId);
    const point = pointerCanvas(e);
    setDrag({ kind: 'connect', from: id, fromSide, currentX: point.x, currentY: point.y });
  };

  const onPointerMove = (e: PointerEvent) => {
    const d = drag();
    if (!d || !surface) return;

    if ('startX' in d && typeof d.startX === 'number') {
      captureIfMoved(e, d.startX, d.startY as number);
    }

    if (d.kind === 'pan') {
      setViewport((v) => panBy(v, e.clientX - d.startX, e.clientY - d.startY));
      setDrag({ ...d, startX: e.clientX, startY: e.clientY });
      return;
    }

    if (d.kind === 'move') {
      const zoom = viewport().zoom;
      // Shift constrains to a single axis.
      let dx = (e.clientX - d.startX) / zoom;
      let dy = (e.clientY - d.startY) / zoom;
      if (e.shiftKey) {
        if (Math.abs(dx) > Math.abs(dy)) dy = 0;
        else dx = 0;
      }
      const useSnap = snapping() && !spaceHeld();
      setHistory((h) => ({
        ...h,
        present: moveSelection(d.origin, selected(), snap(dx, useSnap), snap(dy, useSnap)),
      }));
      return;
    }

    if (d.kind === 'resize') {
      const zoom = viewport().zoom;
      const node = d.origin.nodes.find((n) => n.id === d.id);
      if (!node) return;
      const useSnap = snapping() && !spaceHeld();
      const dx = (e.clientX - d.startX) / zoom;
      const dy = (e.clientY - d.startY) / zoom;

      // The opposite corner is the anchor: dragging the north-west handle moves
      // the origin and shrinks the box, rather than moving the whole card.
      const west = d.corner === 'nw' || d.corner === 'sw';
      const north = d.corner === 'nw' || d.corner === 'ne';
      const x = snap(west ? node.x + dx : node.x, useSnap);
      const y = snap(north ? node.y + dy : node.y, useSnap);

      setHistory((h) => ({
        ...h,
        present: resizeNode(d.origin, d.id, {
          x,
          y,
          width: snap(west ? node.x + node.width - x : node.width + dx, useSnap),
          height: snap(north ? node.y + node.height - y : node.height + dy, useSnap),
        }),
      }));
      return;
    }

    if (d.kind === 'marquee' || d.kind === 'connect') {
      const point = pointerCanvas(e);
      setDrag({ ...d, currentX: point.x, currentY: point.y });
    }
  };

  const onPointerUp = (e: PointerEvent) => {
    const d = drag();
    pendingCapture = null;
    setDrag(null);
    if (!d) return;

    if (d.kind === 'move' || d.kind === 'resize') {
      // The live drag mutated `present` directly so the card tracked the
      // cursor; commit once at the end so undo steps are whole gestures
      // rather than one entry per pointermove.
      const settled = doc();
      setHistory((h) => commit({ ...h, present: d.origin }, settled));
      scheduleSave();
      return;
    }

    if (d.kind === 'marquee') {
      const rect = {
        x: Math.min(d.startX, d.currentX),
        y: Math.min(d.startY, d.currentY),
        width: Math.abs(d.currentX - d.startX),
        height: Math.abs(d.currentY - d.startY),
      };
      if (rect.width > 4 || rect.height > 4) {
        setSelected(new Set(nodesInRect(doc(), rect)));
      }
      return;
    }

    if (d.kind === 'connect') {
      const target = hitTest(doc(), d.currentX, d.currentY);
      if (target && target.id !== d.from) {
        mutate(
          addEdge(doc(), {
            id: uid('edge'),
            fromNode: d.from,
            fromSide: d.fromSide,
            toNode: target.id,
          }),
        );
      }
      void e;
    }
  };

  const hitTest = (document: CanvasDoc, x: number, y: number): CanvasNode | undefined => {
    // Reverse order: the topmost node in z-order wins.
    return [...document.nodes]
      .reverse()
      .find((n) => x >= n.x && x <= n.x + n.width && y >= n.y && y <= n.y + n.height);
  };

  // --- keyboard -------------------------------------------------------------

  const [spaceHeld, setSpaceHeld] = createSignal(false);

  const onKeyDown = (e: KeyboardEvent) => {
    const target = e.target as HTMLElement | null;
    if (target?.isContentEditable || ['INPUT', 'TEXTAREA'].includes(target?.tagName ?? '')) return;

    if (e.key === 'Escape' && editingId()) {
      setEditingId(null);
      surface?.focus();
      return;
    }
    if (e.code === 'Space') {
      setSpaceHeld(true);
      setSnapping(false);
      return;
    }

    const mod = e.ctrlKey || e.metaKey;

    if (mod && e.key.toLowerCase() === 'z') {
      e.preventDefault();
      setHistory((h) => (e.shiftKey ? redo(h) : undo(h)));
      scheduleSave();
      return;
    }
    if (mod && e.key.toLowerCase() === 'a') {
      e.preventDefault();
      setSelected(new Set(doc().nodes.map((n) => n.id)));
      return;
    }
    if (mod && e.key.toLowerCase() === 's') {
      e.preventDefault();
      void save();
      return;
    }
    if (mod && e.key.toLowerCase() === 'g' && selected().size > 0) {
      e.preventDefault();
      mutate(groupSelection(doc(), selected(), uid('group'), 'Group'));
      return;
    }
    if (e.key === 'Delete' || e.key === 'Backspace') {
      const edge = selectedEdge();
      if (edge) {
        e.preventDefault();
        mutate(removeEdges(doc(), new Set([edge])));
        setSelectedEdge(null);
        return;
      }
      if (selected().size > 0) {
        e.preventDefault();
        mutate(removeNodes(doc(), selected()));
        setSelected(new Set<string>());
        return;
      }
    }
    if (e.shiftKey && e.key === '!') {
      frameAll();
      return;
    }
    if (e.shiftKey && e.key === '@') {
      frameSelection();
    }
  };

  const onKeyUp = (e: KeyboardEvent) => {
    if (e.code === 'Space') {
      setSpaceHeld(false);
      setSnapping(true);
    }
  };

  // Bound to the surface rather than `window`: canvas shortcuts are destructive
  // (Delete removes nodes, Ctrl+Z rewinds the document, Ctrl+S writes it), and a
  // window listener fires them while the user is focused on the file tree, a
  // ribbon button, or another pane entirely. With two canvas tabs open, every
  // one of them would also act on a single keypress.

  const createTextNode = (e: MouseEvent) => {
    if (!surface) return;
    const insideNode = (e.target as HTMLElement | null)?.closest?.('[data-node-id]');
    if (insideNode) return;
    const rect = surface.getBoundingClientRect();
    const point = screenToCanvas(viewport(), e.clientX - rect.left, e.clientY - rect.top);
    const node: CanvasNode = {
      id: uid('node'),
      type: 'text',
      text: '',
      x: snap(point.x - 125, snapping()),
      y: snap(point.y - 60, snapping()),
      width: 250,
      height: 120,
    };
    mutate(addNode(doc(), node));
    setSelected(new Set([node.id]));
  };

  // --- render ---------------------------------------------------------------

  const mounted = createMemo(() => visibleNodes(doc().nodes, viewport()));
  // Ids, not node objects: a reference-keyed list rebuilds every card whenever
  // the immutable document produces new objects, which is every drag frame.
  const groupIds = createMemo(() =>
    mounted()
      .filter((n) => n.type === 'group')
      .map((n) => n.id),
  );
  const cardIds = createMemo(() =>
    mounted()
      .filter((n) => n.type !== 'group')
      .map((n) => n.id),
  );
  const nodeById = (id: string) => doc().nodes.find((n) => n.id === id);
  const lowDetail = createMemo(() => isLowDetail(viewport()));

  const absPathFor = (relPath: string) => `${kiln()}/${relPath}`;
  const rawUrlFor = (relPath: string) => rawFileUrl(absPathFor(relPath));

  const openFile = (relPath: string) => {
    void openNoteInEditor(`${kiln()}/${relPath}`);
  };

  return (
    <PanelShell>
      <div class="flex items-center gap-2 border-b border-hairline px-3 py-1.5 text-xs">
        <span class="truncate font-medium text-shell-ink">
          {props.filePath?.split('/').pop() ?? 'Canvas'}
        </span>
        <Show when={dirty()}>
          <span class="text-primary" title="Unsaved changes">
            ●
          </span>
        </Show>
        <div class="ml-auto flex items-center gap-1 text-muted-dark">
          <button
            type="button"
            class="rounded px-1.5 py-0.5 hover:bg-hover-wash disabled:opacity-40"
            disabled={!canUndo(history())}
            onClick={() => {
              setHistory(undo);
              scheduleSave();
            }}
          >
            Undo
          </button>
          <button
            type="button"
            class="rounded px-1.5 py-0.5 hover:bg-hover-wash disabled:opacity-40"
            disabled={!canRedo(history())}
            onClick={() => {
              setHistory(redo);
              scheduleSave();
            }}
          >
            Redo
          </button>
          <button
            type="button"
            class="rounded px-1.5 py-0.5 hover:bg-hover-wash"
            title="Add a card referencing a note"
            data-testid="canvas-add-note"
            onClick={() => setPickingNote(true)}
          >
            + Note
          </button>
          <button type="button" class="rounded px-1.5 py-0.5 hover:bg-hover-wash" onClick={frameAll}>
            Fit
          </button>
          <span class="tabular-nums">{Math.round(viewport().zoom * 100)}%</span>
        </div>
      </div>

      <Show when={error()}>
        <div class="px-3 py-2 text-xs text-error" data-testid="canvas-error">
          {error()}
        </div>
      </Show>

      <div
        ref={surface}
        class="relative min-h-0 flex-1 overflow-hidden outline-none"
        classList={{ 'cursor-grab': spaceHeld() }}
        data-testid="canvas-surface"
        onWheel={onWheel}
        onPointerDown={onSurfacePointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        // Touch and pen deliver pointercancel INSTEAD of pointerup when the
        // gesture is interrupted. Without this the drag state sticks and every
        // later move keeps dragging with no button held.
        onPointerCancel={onPointerUp}
        onKeyDown={onKeyDown}
        onKeyUp={onKeyUp}
        tabindex={0}
        onDblClick={createTextNode}
      >
        <Show when={pickingNote()}>
          <NotePicker
            kiln={kiln()}
            onClose={() => setPickingNote(false)}
            onPick={(rel) => {
              setPickingNote(false);
              const centre = screenToCanvas(
                viewport(),
                viewport().width / 2,
                viewport().height / 2,
              );
              const node: CanvasNode = {
                id: uid('node'),
                type: 'file',
                file: rel,
                x: snap(centre.x - 200, snapping()),
                y: snap(centre.y - 140, snapping()),
                width: 400,
                height: 280,
              };
              mutate(addNode(doc(), node));
              setSelected(new Set<string>([node.id]));
            }}
          />
        </Show>

        <Show when={loading()}>
          <div class="absolute inset-0 grid place-items-center text-xs text-muted">Loading…</div>
        </Show>

        {/* One transformed layer in CANVAS coordinates. Scaling here rather
            than multiplying each node's pixel size means text, padding and
            borders zoom with the cards, instead of staying at a fixed size
            while the boxes around them grow. */}
        <div
          class="absolute left-0 top-0 origin-top-left"
          style={{
            transform: `scale(${viewport().zoom}) translate(${-viewport().x}px, ${-viewport().y}px)`,
          }}
          data-testid="canvas-layer"
        >


          <EdgeToolbar
            doc={doc()}
            edgeId={selectedEdge()}
            onChange={(patch) => mutate(updateEdge(doc(), selectedEdge()!, patch))}
            onDelete={() => {
              mutate(removeEdges(doc(), new Set([selectedEdge()!])));
              setSelectedEdge(null);
            }}
          />

        <For each={groupIds()}>
          {(nodeId) => {
            // An accessor, not a value: the callback runs once per id, but the
            // node object is replaced on every edit, so geometry has to be read
            // reactively or a dragged card would never move.
            const node = () => nodeById(nodeId);
            const isGroup = () => node()?.type === 'group';
            const accent = () => resolveCanvasColor(node()!.color);

            return (
              <div
                class="absolute origin-top-left rounded-lg transition-colors"
                classList={{
                  // Every non-group card gets identical chrome, whatever it
                  // holds — an image card and a text card differing in border
                  // or shadow made the same object look like two kinds.
                  'shadow-sm': !isGroup(),
                  'canvas-group': isGroup(),
                  'canvas-card': !isGroup(),
                  // Selection thickens and brightens the card's OWN border
                  // rather than replacing it with an accent outline, so a
                  // coloured card keeps its colour while selected.
                  'canvas-card-selected': selected().has(node()!.id),
                }}
                style={{
                  left: `${node()!.x}px`,
                  top: `${node()!.y}px`,
                  width: `${node()!.width}px`,
                  height: `${node()!.height}px`,
                  // A card's colour tints its background as well as its edge:
                  // a bare outline reads as an annotation on the card, where
                  // the colour is meant to categorise the card itself.
                  '--canvas-accent': accent() ?? 'var(--color-hairline)',
                  // A separate selection accent: an uncoloured card's border is
                  // deliberately faint, and brightening *that* left selection
                  // almost invisible on the default cards, which are most of
                  // them. A coloured card still selects in its own colour.
                  '--canvas-select': accent() ?? 'var(--color-primary)',
                  // Groups must not eat pointer events meant for their contents.
                  'pointer-events': isGroup() ? 'none' : 'auto',
                }}
                data-testid="canvas-node"
                data-node-id={node()!.id}
                data-node-type={node()!.type}
                data-low-detail={lowDetail() ? 'true' : 'false'}
                onPointerDown={(e) => onNodePointerDown(e, node()!.id)}
                onDblClick={(e) => {
                  e.stopPropagation();
                  if (!isGroup()) setEditingId(node()!.id);
                }}
              >
                <Show when={isGroup() && (node()! as GroupNode).label}>
                  <span
                    class="absolute -top-5 left-0 text-xs font-medium text-muted"
                    style={{ 'pointer-events': 'auto' }}
                  >
                    {(node()! as GroupNode).label}
                  </span>
                </Show>

                <Show
                  when={!lowDetail()}
                  fallback={
                    // The LOD placeholder. Applied to every node type including
                    // media — exempting media is exactly what makes zoomed-out
                    // Obsidian canvases slow.
                    <div
                      class="h-full w-full rounded-lg bg-surface-elevated"
                      data-testid="canvas-node-placeholder"
                    />
                  }
                >
                  <div
                    class="h-full w-full overflow-auto rounded-lg"
                    data-canvas-scroll
                  >
                    <CanvasNodeView
                      node={node()!}
                      rejectedReason={rejectedFor(node()!.id)}
                      rawUrlFor={rawUrlFor}
                      absPathFor={absPathFor}
                      kiln={kiln()}
                      editable={editingId() === node()!.id}
                      onTextChange={(id, text) =>
                        mutate(updateNode(doc(), id, { text } as Partial<CanvasNode>))
                      }
                      onOpenFile={openFile}
                    />
                  </div>
                </Show>

                <CanvasCardChrome
                  selected={selected().has(node()!.id) && !lowDetail()}
                  editing={editingId() === node()!.id}
                  color={node()!.color}
                  onResizeStart={(e, corner) => onResizePointerDown(e, node()!.id, corner)}
                  onConnectStart={(e, side) => onConnectPointerDown(e, node()!.id, side)}
                  canEdit={node()!.type === 'text' || (node()!.type === 'file' && /\.(md|markdown)$/i.test((node()! as { file?: string }).file ?? ''))}
                  onEdit={() => setEditingId(node()!.id)}
                  onColor={(color) => mutate(updateNode(doc(), node()!.id, { color } as Partial<CanvasNode>))}
                  onDelete={() => {
                    mutate(removeNodes(doc(), new Set([node()!.id])));
                    setSelected(new Set<string>());
                  }}
                />
              </div>
            );
          }}
        </For>

          <EdgeLayer
            doc={doc()}
            selectedEdge={selectedEdge()}
            onSelectEdge={(id) => {
              setSelectedEdge(id);
              setSelected(new Set<string>());
              setEditingId(null);
            }}
            pending={drag()?.kind === 'connect' ? (drag() as Extract<Drag, { kind: 'connect' }>) : undefined}
          />

        <For each={cardIds()}>
          {(nodeId) => {
            // An accessor, not a value: the callback runs once per id, but the
            // node object is replaced on every edit, so geometry has to be read
            // reactively or a dragged card would never move.
            const node = () => nodeById(nodeId);
            const isGroup = () => node()?.type === 'group';
            const accent = () => resolveCanvasColor(node()!.color);

            return (
              <div
                class="absolute origin-top-left rounded-lg transition-colors"
                classList={{
                  // Every non-group card gets identical chrome, whatever it
                  // holds — an image card and a text card differing in border
                  // or shadow made the same object look like two kinds.
                  'shadow-sm': !isGroup(),
                  'canvas-group': isGroup(),
                  'canvas-card': !isGroup(),
                  // Selection thickens and brightens the card's OWN border
                  // rather than replacing it with an accent outline, so a
                  // coloured card keeps its colour while selected.
                  'canvas-card-selected': selected().has(node()!.id),
                }}
                style={{
                  left: `${node()!.x}px`,
                  top: `${node()!.y}px`,
                  width: `${node()!.width}px`,
                  height: `${node()!.height}px`,
                  // A card's colour tints its background as well as its edge:
                  // a bare outline reads as an annotation on the card, where
                  // the colour is meant to categorise the card itself.
                  '--canvas-accent': accent() ?? 'var(--color-hairline)',
                  // A separate selection accent: an uncoloured card's border is
                  // deliberately faint, and brightening *that* left selection
                  // almost invisible on the default cards, which are most of
                  // them. A coloured card still selects in its own colour.
                  '--canvas-select': accent() ?? 'var(--color-primary)',
                  // Groups must not eat pointer events meant for their contents.
                  'pointer-events': isGroup() ? 'none' : 'auto',
                }}
                data-testid="canvas-node"
                data-node-id={node()!.id}
                data-node-type={node()!.type}
                data-low-detail={lowDetail() ? 'true' : 'false'}
                onPointerDown={(e) => onNodePointerDown(e, node()!.id)}
                onDblClick={(e) => {
                  e.stopPropagation();
                  if (!isGroup()) setEditingId(node()!.id);
                }}
              >
                <Show when={isGroup() && (node()! as GroupNode).label}>
                  <span
                    class="absolute -top-5 left-0 text-xs font-medium text-muted"
                    style={{ 'pointer-events': 'auto' }}
                  >
                    {(node()! as GroupNode).label}
                  </span>
                </Show>

                <Show
                  when={!lowDetail()}
                  fallback={
                    // The LOD placeholder. Applied to every node type including
                    // media — exempting media is exactly what makes zoomed-out
                    // Obsidian canvases slow.
                    <div
                      class="h-full w-full rounded-lg bg-surface-elevated"
                      data-testid="canvas-node-placeholder"
                    />
                  }
                >
                  <div
                    class="h-full w-full overflow-auto rounded-lg"
                    data-canvas-scroll
                  >
                    <CanvasNodeView
                      node={node()!}
                      rejectedReason={rejectedFor(node()!.id)}
                      rawUrlFor={rawUrlFor}
                      absPathFor={absPathFor}
                      kiln={kiln()}
                      editable={editingId() === node()!.id}
                      onTextChange={(id, text) =>
                        mutate(updateNode(doc(), id, { text } as Partial<CanvasNode>))
                      }
                      onOpenFile={openFile}
                    />
                  </div>
                </Show>

                <CanvasCardChrome
                  selected={selected().has(node()!.id) && !lowDetail()}
                  editing={editingId() === node()!.id}
                  color={node()!.color}
                  onResizeStart={(e, corner) => onResizePointerDown(e, node()!.id, corner)}
                  onConnectStart={(e, side) => onConnectPointerDown(e, node()!.id, side)}
                  canEdit={node()!.type === 'text' || (node()!.type === 'file' && /\.(md|markdown)$/i.test((node()! as { file?: string }).file ?? ''))}
                  onEdit={() => setEditingId(node()!.id)}
                  onColor={(color) => mutate(updateNode(doc(), node()!.id, { color } as Partial<CanvasNode>))}
                  onDelete={() => {
                    mutate(removeNodes(doc(), new Set([node()!.id])));
                    setSelected(new Set<string>());
                  }}
                />
              </div>
            );
          }}
        </For>

          <Show when={drag()?.kind === 'marquee'}>
            {(() => {
              const d = drag() as Extract<Drag, { kind: 'marquee' }>;
              return (
                <div
                  class="pointer-events-none absolute border border-primary bg-primary/10"
                  style={{
                    left: `${Math.min(d.startX, d.currentX)}px`,
                    top: `${Math.min(d.startY, d.currentY)}px`,
                    width: `${Math.abs(d.currentX - d.startX)}px`,
                    height: `${Math.abs(d.currentY - d.startY)}px`,
                  }}
                  data-testid="canvas-marquee"
                />
              );
            })()}
          </Show>
        </div>
      </div>
    </PanelShell>
  );
};

/** All edges in one SVG overlay, redrawn from the current viewport. */
const EdgeLayer: Component<{
  doc: CanvasDoc;
  selectedEdge?: string | null;
  onSelectEdge?: (id: string) => void;
  pending?: { from: string; currentX: number; currentY: number };
}> = (props) => {
  const nodeById = createMemo(() => new Map(props.doc.nodes.map((n) => [n.id, n])));

  const geometry = createMemo(() =>
    props.doc.edges.flatMap((edge: CanvasEdge) => {
      const from = nodeById().get(edge.fromNode);
      const to = nodeById().get(edge.toNode);
      // A dangling endpoint draws nothing rather than throwing — a hand-edited
      // file can name a node that is not there.
      if (!from || !to) return [];

      const fromSide = edge.fromSide ?? inferSide(from, to);
      const toSide = edge.toSide ?? inferSide(to, from);
      const a = anchorPoint(from, fromSide);
      const b = anchorPoint(to, toSide);

      return [
        {
          edge,
          d: edgePath(a, fromSide, b, toSide),
          mid: { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 },
        },
      ];
    }),
  );

  return (
    <svg
      class="pointer-events-none absolute left-0 top-0 h-px w-px overflow-visible"
      // The community perf patch for Obsidian's canvas sets exactly this;
      // edge curves gain nothing from geometric-precision rendering.
      shape-rendering="optimizeSpeed"
      data-testid="canvas-edges"
    >
      <defs>
        <marker id="canvas-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse">
          <path d="M 0 0 L 10 5 L 0 10 z" fill="context-stroke" />
        </marker>
      </defs>

      <For each={geometry()}>
        {(item) => (
          <g style={{ color: resolveCanvasColor(item.edge.color) ?? 'var(--color-muted-dark)' }}>
            {/* A 12px transparent stroke under the visible curve: the drawn
                line is too thin to click reliably. */}
            <path
              d={item.d}
              fill="none"
              stroke="transparent"
              stroke-width={12}
              style={{ 'pointer-events': 'stroke', cursor: 'pointer' }}
              data-testid="canvas-edge-hit"
              data-edge-id={item.edge.id}
              onPointerDown={(e) => {
                e.stopPropagation();
                props.onSelectEdge?.(item.edge.id);
              }}
            />
            <path
              d={item.d}
              fill="none"
              stroke="currentColor"
              stroke-width={props.selectedEdge === item.edge.id ? 2.5 : 1.5}
              marker-start={fromEndOf(item.edge) === 'arrow' ? 'url(#canvas-arrow)' : undefined}
              marker-end={toEndOf(item.edge) === 'arrow' ? 'url(#canvas-arrow)' : undefined}
              data-testid="canvas-edge"
              data-edge-id={item.edge.id}
            />
            <Show when={item.edge.label}>
              {/* Painted behind the text in the canvas background colour, so
                  the label reads as sitting ON the line rather than crossed
                  out by it. Same colour as the surface, not a tinted chip. */}
              <text
                x={item.mid.x}
                y={item.mid.y}
                text-anchor="middle"
                dominant-baseline="middle"
                class="canvas-edge-label-halo text-[11px]"
                data-testid="canvas-edge-label"
              >
                {item.edge.label}
              </text>
              <text
                x={item.mid.x}
                y={item.mid.y}
                text-anchor="middle"
                dominant-baseline="middle"
                class="fill-current text-[11px]"
              >
                {item.edge.label}
              </text>
            </Show>
          </g>
        )}
      </For>

      <Show when={props.pending}>
        {(pending) => {
          const from = nodeById().get(pending().from);
          if (!from) return null;
          const a = { x: from.x + from.width, y: from.y + from.height / 2 };
          const b = { x: pending().currentX, y: pending().currentY };
          return (
            <path
              d={`M ${a.x} ${a.y} L ${b.x} ${b.y}`}
              stroke="var(--color-primary)"
              stroke-width={1.5}
              stroke-dasharray="4 3"
              fill="none"
              data-testid="canvas-pending-edge"
            />
          );
        }}
      </Show>
    </svg>
  );
};

/**
 * Toolbar for a selected edge.
 *
 * An arrow carries a colour, a label and two independent ends, none of which
 * were reachable once the edge was drawn — you could create a connection and
 * then never touch it again.
 */
const EdgeToolbar: Component<{
  doc: CanvasDoc;
  edgeId: string | null;
  onChange: (patch: Partial<CanvasEdge>) => void;
  onDelete: () => void;
}> = (props) => {
  const [paletteOpen, setPaletteOpen] = createSignal(false);
  const [labelOpen, setLabelOpen] = createSignal(false);

  const edge = createMemo(() => props.doc.edges.find((e) => e.id === props.edgeId));
  const anchor = createMemo(() => {
    const e = edge();
    if (!e) return null;
    const from = props.doc.nodes.find((n) => n.id === e.fromNode);
    const to = props.doc.nodes.find((n) => n.id === e.toNode);
    if (!from || !to) return null;
    const a = anchorPoint(from, e.fromSide ?? inferSide(from, to));
    const b = anchorPoint(to, e.toSide ?? inferSide(to, from));
    return { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 };
  });

  return (
    <Show when={edge() && anchor()}>
      <div
        class="absolute z-30"
        style={{ left: `${anchor()!.x}px`, top: `${anchor()!.y}px` }}
        data-testid="canvas-edge-toolbar-anchor"
      >
        <ToolbarShell testid="canvas-edge-toolbar">
          <ToolbarButton
            title="Colour"
            testid="canvas-edge-color"
            active={paletteOpen()}
            onClick={() => setPaletteOpen((v) => !v)}
          >
            <Palette class="h-3.5 w-3.5" />
          </ToolbarButton>
          <ToolbarButton
            title="Edit label"
            testid="canvas-edge-label-btn"
            active={labelOpen()}
            onClick={() => setLabelOpen((v) => !v)}
          >
            <Pencil class="h-3.5 w-3.5" />
          </ToolbarButton>
          <ToolbarButton
            title="Arrowhead at source"
            testid="canvas-edge-from-end"
            active={fromEndOf(edge()!) === 'arrow'}
            onClick={() =>
              props.onChange({ fromEnd: fromEndOf(edge()!) === 'arrow' ? 'none' : 'arrow' })
            }
          >
            <ArrowLeft class="h-3.5 w-3.5" />
          </ToolbarButton>
          <ToolbarButton
            title="Arrowhead at target"
            testid="canvas-edge-to-end"
            active={toEndOf(edge()!) === 'arrow'}
            onClick={() =>
              props.onChange({ toEnd: toEndOf(edge()!) === 'arrow' ? 'none' : 'arrow' })
            }
          >
            <ArrowRight class="h-3.5 w-3.5" />
          </ToolbarButton>
          <ToolbarButton title="Delete" testid="canvas-edge-delete" onClick={props.onDelete}>
            <Trash2 class="h-3.5 w-3.5" />
          </ToolbarButton>

          <Show when={paletteOpen()}>
            <SwatchRow
              color={edge()!.color}
              onColor={(color) => {
                props.onChange({ color });
                setPaletteOpen(false);
              }}
            />
          </Show>
          <Show when={labelOpen()}>
            <input
              class="ml-1 w-36 rounded border border-hairline bg-shell-panel px-1.5 py-0.5 text-xs text-shell-ink outline-none focus:border-primary"
              placeholder="Label…"
              value={edge()!.label ?? ''}
              data-testid="canvas-edge-label-input"
              onInput={(e) => props.onChange({ label: e.currentTarget.value || undefined })}
              onKeyDown={(e) => {
                e.stopPropagation();
                if (e.key === 'Enter' || e.key === 'Escape') setLabelOpen(false);
              }}
            />
          </Show>
        </ToolbarShell>
      </div>
    </Show>
  );
};

export default CanvasPanel;
