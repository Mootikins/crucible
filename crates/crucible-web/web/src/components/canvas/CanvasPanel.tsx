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
  canvasToScreen,
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
  canRedo,
  canUndo,
  commit,
  duplicateNodes,
  groupSelection,
  initHistory,
  moveSelection,
  nodesInRect,
  redo,
  removeNodes,
  resizeNode,
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
  | { kind: 'resize'; id: string; startX: number; startY: number; origin: CanvasDoc }
  | { kind: 'connect'; from: string; currentX: number; currentY: number };

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
  const [drag, setDrag] = createSignal<Drag | null>(null);
  const [snapping, setSnapping] = createSignal(true);

  let surface: HTMLDivElement | undefined;

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
        setHistory(initHistory(res.canvas));
        setRejected(res.rejected);
        setKiln(res.kiln);
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
  onCleanup(() => clearTimeout(saveTimer));

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
    surface.setPointerCapture(e.pointerId);
    const point = pointerCanvas(e);

    // Middle button or space-drag pans; anything else on empty space marquees.
    if (e.button === 1 || spaceHeld()) {
      setDrag({ kind: 'pan', startX: e.clientX, startY: e.clientY });
      return;
    }

    if (!e.shiftKey) setSelected(new Set<string>());
    setDrag({ kind: 'marquee', startX: point.x, startY: point.y, currentX: point.x, currentY: point.y });
  };

  const onNodePointerDown = (e: PointerEvent, id: string) => {
    e.stopPropagation();
    if (!surface) return;
    surface.setPointerCapture(e.pointerId);

    setSelected((prev) => {
      const next = new Set<string>(e.shiftKey ? prev : []);
      if (e.shiftKey && prev.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

    // Alt-drag duplicates rather than moves, matching Obsidian.
    if (e.altKey) {
      const { doc: next, created } = duplicateNodes(doc(), new Set([id]), (i) => uid(`node${i}`));
      mutate(next);
      setSelected(new Set<string>(created));
    }

    setDrag({ kind: 'move', startX: e.clientX, startY: e.clientY, origin: doc() });
  };

  const onResizePointerDown = (e: PointerEvent, id: string) => {
    e.stopPropagation();
    surface?.setPointerCapture(e.pointerId);
    setDrag({ kind: 'resize', id, startX: e.clientX, startY: e.clientY, origin: doc() });
  };

  const onConnectPointerDown = (e: PointerEvent, id: string) => {
    e.stopPropagation();
    surface?.setPointerCapture(e.pointerId);
    const point = pointerCanvas(e);
    setDrag({ kind: 'connect', from: id, currentX: point.x, currentY: point.y });
  };

  const onPointerMove = (e: PointerEvent) => {
    const d = drag();
    if (!d || !surface) return;

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
      setHistory((h) => ({
        ...h,
        present: resizeNode(d.origin, d.id, {
          x: node.x,
          y: node.y,
          width: snap(node.width + (e.clientX - d.startX) / zoom, useSnap),
          height: snap(node.height + (e.clientY - d.startY) / zoom, useSnap),
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
        mutate(addEdge(doc(), { id: uid('edge'), fromNode: d.from, toNode: target.id }));
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
    if ((e.key === 'Delete' || e.key === 'Backspace') && selected().size > 0) {
      e.preventDefault();
      mutate(removeNodes(doc(), selected()));
      setSelected(new Set<string>());
      return;
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

  onMount(() => {
    window.addEventListener('keydown', onKeyDown);
    window.addEventListener('keyup', onKeyUp);
    onCleanup(() => {
      window.removeEventListener('keydown', onKeyDown);
      window.removeEventListener('keyup', onKeyUp);
    });
  });

  const createTextNode = (e: MouseEvent) => {
    if (!surface || e.target !== surface) return;
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
  const lowDetail = createMemo(() => isLowDetail(viewport()));

  const absPathFor = (relPath: string) => `${kiln()}/${relPath}`;
  const rawUrlFor = (relPath: string) => rawFileUrl(absPathFor(relPath));

  const openFile = (relPath: string) => {
    void openNoteInEditor(`${kiln()}/${relPath}`);
  };

  return (
    <PanelShell>
      <div class="flex items-center gap-2 border-b border-subtle px-3 py-1.5 text-xs">
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
        class="relative min-h-0 flex-1 overflow-hidden"
        classList={{ 'cursor-grab': spaceHeld() }}
        data-testid="canvas-surface"
        onWheel={onWheel}
        onPointerDown={onSurfacePointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onDblClick={createTextNode}
      >
        <Show when={loading()}>
          <div class="absolute inset-0 grid place-items-center text-xs text-muted">Loading…</div>
        </Show>

        <EdgeLayer
          doc={doc()}
          viewport={viewport()}
          pending={drag()?.kind === 'connect' ? (drag() as Extract<Drag, { kind: 'connect' }>) : undefined}
        />

        <For each={mounted()}>
          {(node) => {
            const screen = () => canvasToScreen(viewport(), node.x, node.y);
            const isGroup = node.type === 'group';
            const accent = () => resolveCanvasColor(node.color);

            return (
              <div
                class="absolute origin-top-left rounded-lg border transition-shadow"
                classList={{
                  'border-subtle bg-surface-elevated shadow-sm': !isGroup,
                  'border-dashed border-muted-dark/50 bg-transparent': isGroup,
                  'ring-2 ring-primary': selected().has(node.id),
                }}
                style={{
                  left: `${screen().x}px`,
                  top: `${screen().y}px`,
                  width: `${node.width * viewport().zoom}px`,
                  height: `${node.height * viewport().zoom}px`,
                  'border-color': accent(),
                  // Groups must not eat pointer events meant for their contents.
                  'pointer-events': isGroup ? 'none' : 'auto',
                }}
                data-testid="canvas-node"
                data-node-id={node.id}
                data-node-type={node.type}
                data-low-detail={lowDetail() ? 'true' : 'false'}
                onPointerDown={(e) => onNodePointerDown(e, node.id)}
              >
                <Show when={isGroup && (node as GroupNode).label}>
                  <span
                    class="absolute -top-5 left-0 text-xs font-medium text-muted"
                    style={{ 'pointer-events': 'auto' }}
                  >
                    {(node as GroupNode).label}
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
                  <div class="h-full w-full overflow-hidden rounded-lg">
                    <CanvasNodeView
                      node={node}
                      rejectedReason={rejectedFor(node.id)}
                      rawUrlFor={rawUrlFor}
                      absPathFor={absPathFor}
                      editable={selected().has(node.id) && selected().size === 1}
                      onOpenFile={openFile}
                    />
                  </div>
                </Show>

                <Show when={selected().has(node.id) && !lowDetail()}>
                  {/* Resize grip */}
                  <div
                    class="absolute -bottom-1 -right-1 h-3 w-3 cursor-nwse-resize rounded-sm border border-primary bg-surface-elevated"
                    style={{ 'pointer-events': 'auto' }}
                    data-testid="canvas-resize-handle"
                    onPointerDown={(e) => onResizePointerDown(e, node.id)}
                  />
                  {/* Connection dot */}
                  <div
                    class="absolute -right-1.5 top-1/2 h-3 w-3 -translate-y-1/2 cursor-crosshair rounded-full border border-primary bg-surface-elevated"
                    style={{ 'pointer-events': 'auto' }}
                    data-testid="canvas-connect-handle"
                    onPointerDown={(e) => onConnectPointerDown(e, node.id)}
                  />
                </Show>
              </div>
            );
          }}
        </For>

        <Show when={drag()?.kind === 'marquee'}>
          {(() => {
            const d = drag() as Extract<Drag, { kind: 'marquee' }>;
            const a = canvasToScreen(viewport(), Math.min(d.startX, d.currentX), Math.min(d.startY, d.currentY));
            const w = Math.abs(d.currentX - d.startX) * viewport().zoom;
            const h = Math.abs(d.currentY - d.startY) * viewport().zoom;
            return (
              <div
                class="pointer-events-none absolute border border-primary bg-primary/10"
                style={{ left: `${a.x}px`, top: `${a.y}px`, width: `${w}px`, height: `${h}px` }}
                data-testid="canvas-marquee"
              />
            );
          })()}
        </Show>
      </div>
    </PanelShell>
  );
};

/** All edges in one SVG overlay, redrawn from the current viewport. */
const EdgeLayer: Component<{
  doc: CanvasDoc;
  viewport: Viewport;
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
      const sa = canvasToScreen(props.viewport, a.x, a.y);
      const sb = canvasToScreen(props.viewport, b.x, b.y);

      return [
        {
          edge,
          d: edgePath(sa, fromSide, sb, toSide),
          mid: { x: (sa.x + sb.x) / 2, y: (sa.y + sb.y) / 2 },
        },
      ];
    }),
  );

  return (
    <svg
      class="pointer-events-none absolute inset-0 h-full w-full"
      // The community perf patch for Obsidian's canvas sets exactly this;
      // edge curves gain nothing from geometric-precision rendering.
      shape-rendering="optimizeSpeed"
      data-testid="canvas-edges"
    >
      <defs>
        <marker id="canvas-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse">
          <path d="M 0 0 L 10 5 L 0 10 z" fill="currentColor" />
        </marker>
      </defs>

      <For each={geometry()}>
        {(item) => (
          <g style={{ color: resolveCanvasColor(item.edge.color) ?? 'var(--color-muted-dark)' }}>
            <path
              d={item.d}
              fill="none"
              stroke="currentColor"
              stroke-width={1.5}
              marker-start={fromEndOf(item.edge) === 'arrow' ? 'url(#canvas-arrow)' : undefined}
              marker-end={toEndOf(item.edge) === 'arrow' ? 'url(#canvas-arrow)' : undefined}
              data-testid="canvas-edge"
              data-edge-id={item.edge.id}
            />
            <Show when={item.edge.label}>
              <text
                x={item.mid.x}
                y={item.mid.y}
                text-anchor="middle"
                class="fill-current text-[11px]"
                data-testid="canvas-edge-label"
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
          const a = canvasToScreen(props.viewport, from.x + from.width, from.y + from.height / 2);
          const b = canvasToScreen(props.viewport, pending().currentX, pending().currentY);
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

export default CanvasPanel;
