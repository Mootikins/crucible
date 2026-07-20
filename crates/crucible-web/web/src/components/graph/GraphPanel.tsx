import {
  Component,
  Show,
  createEffect,
  createSignal,
  on,
  onCleanup,
  onMount,
} from 'solid-js';
import { createStore, unwrap } from 'solid-js/store';
import {
  forceCollide,
  forceLink,
  forceManyBody,
  forceSimulation,
  forceX,
  forceY,
  type ForceLink,
  type Simulation,
} from 'd3-force';
import { PanelShell } from '../PanelShell';
import { IconButton } from '../ui/IconButton';
import { Crosshair, Settings } from '@/lib/icons';
import { getConfig, getKilnGraph } from '@/lib/api';
import { statusBarStore } from '@/stores/statusBarStore';
import { openFileInEditor } from '@/lib/file-actions';
import { kilnRoot, noteAbsolutePath } from '@/lib/note-actions';
import {
  BASE_NODE_RADIUS,
  adoptPositions,
  buildAdjacency,
  buildGraph,
  nodeRadius,
} from '@/lib/graph/build';
import {
  DEFAULT_GRAPH_SETTINGS,
  type GraphDto,
  type GraphEdge,
  type GraphNode,
  type GraphSettings,
} from '@/lib/graph/types';
import { GraphControls } from './GraphControls';

const SETTINGS_KEY = 'crucible-graph-settings';

function loadSettings(): GraphSettings {
  try {
    const raw = localStorage.getItem(SETTINGS_KEY);
    if (!raw) return structuredClone(DEFAULT_GRAPH_SETTINGS);
    const stored = JSON.parse(raw) as Partial<GraphSettings>;
    return {
      filters: { ...DEFAULT_GRAPH_SETTINGS.filters, ...stored.filters },
      display: { ...DEFAULT_GRAPH_SETTINGS.display, ...stored.display },
      forces: { ...DEFAULT_GRAPH_SETTINGS.forces, ...stored.forces },
    };
  } catch {
    return structuredClone(DEFAULT_GRAPH_SETTINGS);
  }
}

interface GraphColors {
  note: string;
  phantom: string;
  tag: string;
  accent: string;
  link: string;
  label: string;
}

const cssVar = (name: string, fallback: string): string =>
  getComputedStyle(document.documentElement).getPropertyValue(name).trim() || fallback;

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

/**
 * Obsidian-style knowledge graph: force-directed canvas over the kiln's
 * resolved link index (`/api/kiln/graph`). Physics via d3-force; rendering
 * is a plain 2D canvas redrawn through a dirty-flag rAF loop, so an idle
 * settled graph costs nothing.
 *
 * Interactions: wheel = zoom to cursor, drag background = pan, drag node =
 * pin-while-dragging (reheats the sim), hover = neighborhood highlight,
 * click note = open in editor, double-click background = fit view.
 */
export const GraphPanel: Component = () => {
  const [settings, setSettings] = createStore<GraphSettings>(loadSettings());
  const [dto, setDto] = createSignal<GraphDto | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const [stats, setStats] = createSignal({ notes: 0, links: 0 });
  const [controlsOpen, setControlsOpen] = createSignal(false);
  const [kiln, setKiln] = createSignal<string | null>(null);

  // --- canvas / sim state kept OUT of solid reactivity (mutated per frame) ---
  let wrapEl: HTMLDivElement | undefined;
  let canvasEl: HTMLCanvasElement | undefined;
  let ctx: CanvasRenderingContext2D | null = null;
  let width = 0;
  let height = 0;
  let dpr = 1;
  const view = { x: 0, y: 0, k: 1 };
  let nodes: GraphNode[] = [];
  let edges: GraphEdge[] = [];
  let adjacency = new Map<string, Set<string>>();
  let hoverId: string | null = null;
  // Eased 0..1 hover intensity + the set it applies to. The set is kept from
  // the LAST hovered node so labels/dimming fade out in place instead of
  // snapping when the pointer leaves.
  let hoverT = 0;
  let hoverSet: Set<string> | null = null;
  let dirty = true;
  let raf = 0;
  let didAutoFit = false;
  let colors: GraphColors = {
    note: '#98939e',
    phantom: '#6b6673',
    tag: '#a78bda',
    accent: '#e0653a',
    link: '#322f38',
    label: '#c9c5bf',
  };

  const markDirty = () => {
    dirty = true;
  };

  const sim: Simulation<GraphNode, GraphEdge> = forceSimulation<GraphNode>([])
    .force('link', forceLink<GraphNode, GraphEdge>([]).id((d) => d.id))
    .force('charge', forceManyBody<GraphNode>().distanceMax(600))
    .force('collide', forceCollide<GraphNode>())
    .force('x', forceX<GraphNode>(0))
    .force('y', forceY<GraphNode>(0))
    .velocityDecay(0.35)
    .on('tick', markDirty)
    .stop();

  const linkForce = () => sim.force('link') as ForceLink<GraphNode, GraphEdge>;

  const applyForces = () => {
    const f = settings.forces;
    linkForce().distance(f.linkDistance).strength(f.linkForce * 0.6);
    (sim.force('charge') as ReturnType<typeof forceManyBody<GraphNode>>).strength(
      -60 * f.repelForce,
    );
    // Hard minimum spacing (Obsidian-style): nodes never overlap or crowd,
    // whatever the charge/link settings. Radius tracks the display knob.
    (sim.force('collide') as ReturnType<typeof forceCollide<GraphNode>>)
      .radius(BASE_NODE_RADIUS * settings.display.nodeSize * 2.5)
      .strength(0.9);
    (sim.force('x') as ReturnType<typeof forceX<GraphNode>>).strength(f.centerForce * 0.12);
    (sim.force('y') as ReturnType<typeof forceY<GraphNode>>).strength(f.centerForce * 0.12);
  };

  const rebuild = () => {
    const data = dto();
    if (!data) return;
    const built = buildGraph(data, unwrap(settings).filters);
    adoptPositions(built.nodes, nodes);
    nodes = built.nodes;
    edges = built.edges;
    adjacency = buildAdjacency(edges);
    setStats({
      notes: nodes.filter((n) => n.kind === 'note').length,
      links: edges.length,
    });
    if (hoverId && !adjacency.has(hoverId) && !nodes.some((n) => n.id === hoverId)) {
      hoverId = null;
      hoverSet = null;
      hoverT = 0;
    }
    sim.nodes(nodes);
    linkForce().links(edges);
    applyForces();
    sim.alpha(1).restart();
    markDirty();
  };

  // --- data ---
  onMount(() => {
    void (async () => {
      try {
        const kilnPath =
          statusBarStore.kilnPath() ?? (await getConfig()).kiln_path ?? null;
        if (!kilnPath) {
          setError('No kiln configured');
          return;
        }
        setKiln(kilnRoot(kilnPath));
        setDto(await getKilnGraph(kilnRoot(kilnPath)));
      } catch (e) {
        setError(e instanceof Error ? e.message : 'Failed to load graph');
      }
    })();
  });

  // Rebuild on data arrival or any filter change.
  createEffect(
    on(
      () => [
        dto(),
        settings.filters.query,
        settings.filters.showTags,
        settings.filters.showPhantoms,
        settings.filters.showOrphans,
      ],
      rebuild,
    ),
  );

  // Retune physics live; small reheat so the layout glides to the new params.
  createEffect(
    on(
      () => [
        settings.forces.centerForce,
        settings.forces.repelForce,
        settings.forces.linkForce,
        settings.forces.linkDistance,
      ],
      () => {
        applyForces();
        sim.alpha(0.4).restart();
      },
      { defer: true },
    ),
  );

  // Display knobs repaint; node size also feeds the collide radius, so the
  // layout gently re-spaces when it changes.
  createEffect(
    on(
      () => [settings.display.nodeSize, settings.display.linkThickness],
      () => {
        applyForces();
        sim.alpha(0.3).restart();
      },
      { defer: true },
    ),
  );

  createEffect(() => {
    localStorage.setItem(SETTINGS_KEY, JSON.stringify(unwrap(settings)));
  });

  // --- viewport ---
  const fitView = () => {
    if (nodes.length === 0 || width === 0) return;
    let minX = Infinity;
    let minY = Infinity;
    let maxX = -Infinity;
    let maxY = -Infinity;
    for (const n of nodes) {
      minX = Math.min(minX, n.x ?? 0);
      minY = Math.min(minY, n.y ?? 0);
      maxX = Math.max(maxX, n.x ?? 0);
      maxY = Math.max(maxY, n.y ?? 0);
    }
    const bw = Math.max(maxX - minX, 40);
    const bh = Math.max(maxY - minY, 40);
    view.k = clamp(Math.min(width / bw, height / bh) * 0.85, 0.05, 2.5);
    view.x = width / 2 - ((minX + maxX) / 2) * view.k;
    view.y = height / 2 - ((minY + maxY) / 2) * view.k;
    markDirty();
  };

  // --- drawing ---
  const draw = () => {
    if (!ctx || !canvasEl) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, width, height);
    ctx.translate(view.x, view.y);
    ctx.scale(view.k, view.k);

    const { nodeSize, linkThickness } = settings.display;
    // Everything hover-dependent lerps on the eased hoverT so the highlight
    // (dimming, accent, labels) fades instead of snapping.
    const set = hoverSet;
    const t = hoverT;
    const lerp = (a: number, b: number, x: number) => a + (b - a) * x;
    const inSet = (id: string) => set !== null && set.has(id);

    // Edges, base pass. d3-force has replaced string endpoints with node refs.
    const edgeWidth = Math.max(0.7 * linkThickness, 0.5 / view.k);
    ctx.lineWidth = edgeWidth;
    ctx.strokeStyle = colors.link;
    for (const e of edges) {
      const s = e.source as GraphNode;
      const d = e.target as GraphNode;
      if (typeof s === 'string' || typeof d === 'string') continue;
      const lit = inSet(s.id) && inSet(d.id);
      const base = e.kind === 'tag' ? 0.18 : 0.32;
      ctx.globalAlpha = lerp(base, lit ? 0.15 : 0.05, t);
      ctx.beginPath();
      ctx.moveTo(s.x ?? 0, s.y ?? 0);
      ctx.lineTo(d.x ?? 0, d.y ?? 0);
      ctx.stroke();
    }
    // Accent overlay on neighborhood edges — alpha rides hoverT, so the hue
    // eases in rather than flipping.
    if (t > 0.01 && set) {
      ctx.strokeStyle = colors.accent;
      ctx.globalAlpha = 0.8 * t;
      for (const e of edges) {
        const s = e.source as GraphNode;
        const d = e.target as GraphNode;
        if (typeof s === 'string' || typeof d === 'string') continue;
        if (!(inSet(s.id) && inSet(d.id))) continue;
        ctx.beginPath();
        ctx.moveTo(s.x ?? 0, s.y ?? 0);
        ctx.lineTo(d.x ?? 0, d.y ?? 0);
        ctx.stroke();
      }
    }

    ctx.font = `${11 / view.k}px 'IBM Plex Sans', system-ui, sans-serif`;
    ctx.textAlign = 'center';
    ctx.textBaseline = 'top';

    for (const n of nodes) {
      const r = nodeRadius(n, nodeSize);
      const x = n.x ?? 0;
      const y = n.y ?? 0;
      const isHover = n.id === hoverId && t > 0.01;
      const member = inSet(n.id);

      const base = n.kind === 'phantom' ? 0.45 : 1;
      ctx.globalAlpha = lerp(base, member ? 1 : 0.08, t);
      ctx.fillStyle =
        n.kind === 'tag' ? colors.tag : n.kind === 'phantom' ? colors.phantom : colors.note;
      ctx.beginPath();
      ctx.arc(x, y, r, 0, Math.PI * 2);
      ctx.fill();

      if (isHover) {
        // Accent tint + ring ease in with hoverT.
        ctx.globalAlpha = t;
        ctx.fillStyle = colors.accent;
        ctx.beginPath();
        ctx.arc(x, y, r, 0, Math.PI * 2);
        ctx.fill();
        ctx.globalAlpha = 0.25 * t;
        ctx.strokeStyle = colors.accent;
        ctx.lineWidth = 3 / view.k;
        ctx.beginPath();
        ctx.arc(x, y, r + 3 / view.k, 0, Math.PI * 2);
        ctx.stroke();
        ctx.lineWidth = edgeWidth;
      }

      // Labels are hover-only: the hovered neighborhood's names fade in with
      // the highlight and out after the pointer leaves.
      if (member && t > 0.02) {
        ctx.globalAlpha = t;
        ctx.fillStyle = colors.label;
        ctx.fillText(n.label, x, y + r + 3 / view.k);
      }
    }
    ctx.globalAlpha = 1;
  };

  // --- interactions ---
  type DragState =
    | { mode: 'pan'; sx: number; sy: number; ox: number; oy: number; moved: number }
    | { mode: 'node'; node: GraphNode; moved: number };
  let drag: DragState | null = null;

  const toWorld = (e: PointerEvent | WheelEvent | MouseEvent) => {
    const rect = canvasEl!.getBoundingClientRect();
    const sx = e.clientX - rect.left;
    const sy = e.clientY - rect.top;
    return { sx, sy, wx: (sx - view.x) / view.k, wy: (sy - view.y) / view.k };
  };

  const hitTest = (wx: number, wy: number): GraphNode | null => {
    const slop = 3 / view.k;
    for (let i = nodes.length - 1; i >= 0; i--) {
      const n = nodes[i];
      const r = nodeRadius(n, settings.display.nodeSize) + slop;
      const dx = (n.x ?? 0) - wx;
      const dy = (n.y ?? 0) - wy;
      if (dx * dx + dy * dy <= r * r) return n;
    }
    return null;
  };

  const openNode = (n: GraphNode) => {
    const k = kiln();
    if (n.kind !== 'note' || !n.path || !k) return;
    openFileInEditor(noteAbsolutePath(n.path, k), n.label);
  };

  const onPointerDown = (e: PointerEvent) => {
    if (e.button !== 0) return;
    canvasEl!.setPointerCapture(e.pointerId);
    const { sx, sy, wx, wy } = toWorld(e);
    const hit = hitTest(wx, wy);
    if (hit) {
      drag = { mode: 'node', node: hit, moved: 0 };
      hit.fx = wx;
      hit.fy = wy;
      sim.alphaTarget(0.3).restart();
    } else {
      drag = { mode: 'pan', sx, sy, ox: view.x, oy: view.y, moved: 0 };
    }
  };

  const onPointerMove = (e: PointerEvent) => {
    const { sx, sy, wx, wy } = toWorld(e);
    if (drag?.mode === 'node') {
      drag.moved += Math.abs(e.movementX) + Math.abs(e.movementY);
      drag.node.fx = wx;
      drag.node.fy = wy;
      markDirty();
      return;
    }
    if (drag?.mode === 'pan') {
      drag.moved += Math.abs(e.movementX) + Math.abs(e.movementY);
      view.x = drag.ox + (sx - drag.sx);
      view.y = drag.oy + (sy - drag.sy);
      markDirty();
      return;
    }
    const hit = hitTest(wx, wy);
    const id = hit?.id ?? null;
    if (id !== hoverId) {
      hoverId = id;
      // Keep the previous set on leave so the fade-out has something to fade.
      if (hit) hoverSet = new Set([hit.id, ...(adjacency.get(hit.id) ?? [])]);
      canvasEl!.style.cursor = hit ? 'pointer' : 'grab';
      markDirty();
    }
  };

  const onPointerUp = (e: PointerEvent) => {
    if (!drag) return;
    const wasClick = drag.moved < 5;
    if (drag.mode === 'node') {
      sim.alphaTarget(0);
      drag.node.fx = null;
      drag.node.fy = null;
      if (wasClick) openNode(drag.node);
    }
    drag = null;
    canvasEl!.releasePointerCapture(e.pointerId);
  };

  const onWheel = (e: WheelEvent) => {
    e.preventDefault();
    const { sx, sy, wx, wy } = toWorld(e);
    const factor = e.deltaY < 0 ? 1.15 : 1 / 1.15;
    view.k = clamp(view.k * factor, 0.05, 8);
    view.x = sx - wx * view.k;
    view.y = sy - wy * view.k;
    markDirty();
  };

  const onDblClick = (e: MouseEvent) => {
    const { wx, wy } = toWorld(e);
    if (!hitTest(wx, wy)) fitView();
  };

  // --- lifecycle ---
  onMount(() => {
    if (!canvasEl || !wrapEl) return;
    ctx = canvasEl.getContext('2d');
    colors = {
      note: cssVar('--color-muted', colors.note),
      phantom: cssVar('--color-muted-dark', colors.phantom),
      tag: cssVar('--color-precog', colors.tag),
      accent: cssVar('--color-primary', colors.accent),
      link: cssVar('--color-hairline-strong', colors.link),
      label: cssVar('--color-shell-body', colors.label),
    };
    canvasEl.style.cursor = 'grab';

    const ro = new ResizeObserver(() => {
      const rect = wrapEl!.getBoundingClientRect();
      dpr = window.devicePixelRatio || 1;
      const first = width === 0 && rect.width > 0;
      width = rect.width;
      height = rect.height;
      canvasEl!.width = Math.round(width * dpr);
      canvasEl!.height = Math.round(height * dpr);
      if (first) {
        view.x = width / 2;
        view.y = height / 2;
      }
      markDirty();
    });
    ro.observe(wrapEl);
    onCleanup(() => ro.disconnect());

    const frame = () => {
      // Ease the hover highlight toward its target; animating counts as dirty.
      const target = hoverId ? 1 : 0;
      if (hoverT !== target) {
        const next = hoverT + (target - hoverT) * 0.22;
        hoverT = Math.abs(next - target) < 0.01 ? target : next;
        if (hoverT === 0) hoverSet = null;
        dirty = true;
      }
      if (dirty) {
        dirty = false;
        // Fit once, shortly after the first layout has spread out.
        if (!didAutoFit && nodes.length > 0 && sim.alpha() < 0.5) {
          didAutoFit = true;
          fitView();
        }
        draw();
      }
      raf = requestAnimationFrame(frame);
    };
    raf = requestAnimationFrame(frame);
    onCleanup(() => cancelAnimationFrame(raf));
    onCleanup(() => sim.stop());
  });

  return (
    <PanelShell class="relative overflow-hidden">
      <div ref={wrapEl} class="absolute inset-0">
        <canvas
          ref={canvasEl}
          class="absolute inset-0 w-full h-full touch-none"
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
          onWheel={onWheel}
          onDblClick={onDblClick}
        />
      </div>

      <Show when={error()}>
        <div class="absolute inset-0 flex items-center justify-center">
          <span class="text-sm text-muted">{error()}</span>
        </div>
      </Show>
      <Show when={!error() && !dto()}>
        <div class="absolute inset-0 flex items-center justify-center">
          <span class="text-sm text-muted">Loading graph…</span>
        </div>
      </Show>
      <Show when={dto() && stats().notes === 0}>
        <div class="absolute inset-0 flex items-center justify-center pointer-events-none">
          <span class="text-sm text-muted">
            {settings.filters.query ? 'No notes match the filter' : 'No notes in this kiln yet'}
          </span>
        </div>
      </Show>

      {/* Chrome overlays */}
      <div class="absolute top-2 right-2 flex items-center gap-1">
        <IconButton title="Fit view" aria-label="Fit graph to view" onClick={fitView}>
          <Crosshair class="w-4 h-4" />
        </IconButton>
        <IconButton
          title="Graph settings"
          aria-label="Toggle graph settings"
          onClick={() => setControlsOpen((v) => !v)}
        >
          <Settings class="w-4 h-4" />
        </IconButton>
      </div>
      <Show when={controlsOpen()}>
        <GraphControls settings={settings} onChange={setSettings} />
      </Show>
      <div class="absolute bottom-2 left-3 text-[11px] text-muted-dark pointer-events-none select-none">
        {stats().notes} notes · {stats().links} links
      </div>
    </PanelShell>
  );
};
