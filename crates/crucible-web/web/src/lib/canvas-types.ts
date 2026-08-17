/**
 * JSON Canvas 1.0 — https://jsoncanvas.org
 *
 * Mirrors `crucible_core::canvas`. Unknown keys are preserved on the Rust side
 * and round-trip through here untouched, so anything this file does not model
 * still survives a save. That matters: Obsidian plugins write keys outside the
 * spec, and dropping them would quietly destroy work authored elsewhere.
 */

export type CanvasColor = string;

export type CanvasSide = 'top' | 'right' | 'bottom' | 'left';
export type CanvasEnd = 'none' | 'arrow';
export type CanvasBackgroundStyle = 'cover' | 'ratio' | 'repeat';

interface NodeBase {
  id: string;
  x: number;
  y: number;
  width: number;
  height: number;
  color?: CanvasColor;
  /** Keys outside the spec ride along here on the wire. */
  [key: string]: unknown;
}

export interface TextNode extends NodeBase {
  type: 'text';
  text: string;
}

export interface FileNode extends NodeBase {
  type: 'file';
  /** Kiln-relative. Empty when the server redacted a rejected reference. */
  file: string;
  subpath?: string;
}

export interface LinkNode extends NodeBase {
  type: 'link';
  url: string;
}

export interface GroupNode extends NodeBase {
  type: 'group';
  label?: string;
  background?: string;
  backgroundStyle?: CanvasBackgroundStyle;
}

export type CanvasNode = TextNode | FileNode | LinkNode | GroupNode;

export interface CanvasEdge {
  id: string;
  fromNode: string;
  fromSide?: CanvasSide;
  fromEnd?: CanvasEnd;
  toNode: string;
  toSide?: CanvasSide;
  toEnd?: CanvasEnd;
  color?: CanvasColor;
  label?: string;
  [key: string]: unknown;
}

export interface CanvasDoc {
  nodes: CanvasNode[];
  edges: CanvasEdge[];
  [key: string]: unknown;
}

/** A reference the server refused. The offending path is deliberately absent. */
export interface RejectedRef {
  nodeId: string;
  reason: string;
}

export interface CanvasResponse {
  canvas: CanvasDoc;
  rejected: RejectedRef[];
  kiln: string;
}

/**
 * The six preset slots, mapped to theme tokens rather than raw hex so a canvas
 * reads correctly in both light and dark. A hex colour authored by the user is
 * passed through as-is.
 */
const PRESET_COLORS: Record<string, string> = {
  '1': 'var(--canvas-red, #e5534b)',
  '2': 'var(--canvas-orange, #d98032)',
  '3': 'var(--canvas-yellow, #d9c02f)',
  '4': 'var(--canvas-green, #48a860)',
  '5': 'var(--canvas-cyan, #3d9fb0)',
  '6': 'var(--canvas-purple, #9a5fc4)',
};

export function resolveCanvasColor(color: CanvasColor | undefined): string | undefined {
  if (!color) return undefined;
  return PRESET_COLORS[color] ?? (color.startsWith('#') ? color : undefined);
}

/** Spec defaults are asymmetric: a bare edge is a one-way arrow, not a line. */
export function fromEndOf(edge: CanvasEdge): CanvasEnd {
  return edge.fromEnd ?? 'none';
}
export function toEndOf(edge: CanvasEdge): CanvasEnd {
  return edge.toEnd ?? 'arrow';
}

export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export function nodeRect(node: CanvasNode): Rect {
  return { x: node.x, y: node.y, width: node.width, height: node.height };
}

export function rectsIntersect(a: Rect, b: Rect): boolean {
  return (
    a.x < b.x + b.width && a.x + a.width > b.x && a.y < b.y + b.height && a.y + a.height > b.y
  );
}

function rectContains(outer: Rect, inner: Rect): boolean {
  return (
    inner.x >= outer.x &&
    inner.y >= outer.y &&
    inner.x + inner.width <= outer.x + outer.width &&
    inner.y + inner.height <= outer.y + outer.height
  );
}

/**
 * Nodes geometrically inside a group.
 *
 * JSON Canvas gives groups no child list — membership *is* containment — so
 * every "move the group and its contents" operation has to derive this.
 */
export function groupMembers(doc: CanvasDoc, group: GroupNode): CanvasNode[] {
  const bounds = nodeRect(group);
  return doc.nodes.filter((n) => n.id !== group.id && rectContains(bounds, nodeRect(n)));
}

/** Bounding box of the given nodes, or null when there are none. */
export function boundsOf(nodes: CanvasNode[]): Rect | null {
  if (nodes.length === 0) return null;
  let minX = Infinity;
  let minY = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;
  for (const n of nodes) {
    minX = Math.min(minX, n.x);
    minY = Math.min(minY, n.y);
    maxX = Math.max(maxX, n.x + n.width);
    maxY = Math.max(maxY, n.y + n.height);
  }
  return { x: minX, y: minY, width: maxX - minX, height: maxY - minY };
}

/**
 * Where an edge attaches to a node.
 *
 * When the document does not pin a side, pick the one facing the other node.
 * Obsidian does the same, and hard-coding (say) right→left makes edges cross
 * their own nodes as soon as cards are laid out vertically.
 */
export function anchorPoint(node: CanvasNode, side: CanvasSide): { x: number; y: number } {
  switch (side) {
    case 'top':
      return { x: node.x + node.width / 2, y: node.y };
    case 'bottom':
      return { x: node.x + node.width / 2, y: node.y + node.height };
    case 'left':
      return { x: node.x, y: node.y + node.height / 2 };
    case 'right':
      return { x: node.x + node.width, y: node.y + node.height / 2 };
  }
}

export function inferSide(from: CanvasNode, to: CanvasNode): CanvasSide {
  const dx = to.x + to.width / 2 - (from.x + from.width / 2);
  const dy = to.y + to.height / 2 - (from.y + from.height / 2);
  if (Math.abs(dx) > Math.abs(dy)) return dx > 0 ? 'right' : 'left';
  return dy > 0 ? 'bottom' : 'top';
}

/**
 * A cubic bezier between two anchors, bowed out along each side's normal so
 * the curve leaves and enters perpendicular to the card edge.
 */
export function edgePath(
  from: { x: number; y: number },
  fromSide: CanvasSide,
  to: { x: number; y: number },
  toSide: CanvasSide,
): string {
  const distance = Math.hypot(to.x - from.x, to.y - from.y);
  const bow = Math.max(40, Math.min(160, distance / 2));
  const c1 = offsetAlong(from, fromSide, bow);
  const c2 = offsetAlong(to, toSide, bow);
  return `M ${from.x} ${from.y} C ${c1.x} ${c1.y}, ${c2.x} ${c2.y}, ${to.x} ${to.y}`;
}

function offsetAlong(
  point: { x: number; y: number },
  side: CanvasSide,
  distance: number,
): { x: number; y: number } {
  switch (side) {
    case 'top':
      return { x: point.x, y: point.y - distance };
    case 'bottom':
      return { x: point.x, y: point.y + distance };
    case 'left':
      return { x: point.x - distance, y: point.y };
    case 'right':
      return { x: point.x + distance, y: point.y };
  }
}
