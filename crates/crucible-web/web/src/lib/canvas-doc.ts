/**
 * Canvas document mutations and undo history.
 *
 * Deliberately plain functions over plain data, with no Solid or DOM. A spatial
 * editor accumulates a lot of fiddly geometry rules — group membership, z-order,
 * edge cleanup — and every one of them is easier to get right, and far easier to
 * keep right, when it can be tested without mounting anything.
 *
 * Mutations are immutable: each returns a new document. That is what makes undo
 * a stack of snapshots rather than a set of hand-written inverse operations,
 * each of which would be its own opportunity to corrupt a file.
 */

import {
  boundsOf,
  groupMembers,
  nodeRect,
  rectsIntersect,
  type CanvasDoc,
  type CanvasEdge,
  type CanvasNode,
  type GroupNode,
  type Rect,
} from './canvas-types';

/** Unknown keys ride along untouched; only what we name is replaced. */
function patchNode(node: CanvasNode, patch: Partial<CanvasNode>): CanvasNode {
  return { ...node, ...patch } as CanvasNode;
}

export function updateNode(
  doc: CanvasDoc,
  id: string,
  patch: Partial<CanvasNode>,
): CanvasDoc {
  return { ...doc, nodes: doc.nodes.map((n) => (n.id === id ? patchNode(n, patch) : n)) };
}

export function updateNodes(
  doc: CanvasDoc,
  ids: Set<string>,
  patch: (node: CanvasNode) => Partial<CanvasNode>,
): CanvasDoc {
  return {
    ...doc,
    nodes: doc.nodes.map((n) => (ids.has(n.id) ? patchNode(n, patch(n)) : n)),
  };
}

export function addNode(doc: CanvasDoc, node: CanvasNode): CanvasDoc {
  // Appended, because document order is z-order and a new card belongs on top.
  return { ...doc, nodes: [...doc.nodes, node] };
}

/**
 * Remove nodes, and any edge that touched them.
 *
 * Leaving dangling edges behind would produce a document that still parses but
 * renders nothing for those edges, which is worse than removing them: the user
 * sees no edge and no explanation, and the rot persists in the file.
 */
export function removeNodes(doc: CanvasDoc, ids: Set<string>): CanvasDoc {
  return {
    ...doc,
    nodes: doc.nodes.filter((n) => !ids.has(n.id)),
    edges: doc.edges.filter((e) => !ids.has(e.fromNode) && !ids.has(e.toNode)),
  };
}

export function addEdge(doc: CanvasDoc, edge: CanvasEdge): CanvasDoc {
  return { ...doc, edges: [...doc.edges, edge] };
}

export function updateEdge(doc: CanvasDoc, id: string, patch: Partial<CanvasEdge>): CanvasDoc {
  return { ...doc, edges: doc.edges.map((e) => (e.id === id ? { ...e, ...patch } : e)) };
}

export function removeEdges(doc: CanvasDoc, ids: Set<string>): CanvasDoc {
  return { ...doc, edges: doc.edges.filter((e) => !ids.has(e.id)) };
}

/**
 * Move a selection by a canvas-space delta.
 *
 * Dragging a group carries its geometric members, since JSON Canvas has no
 * child list and the visual promise of a group is that its contents travel with
 * it. Membership is resolved against the document *before* the move so a card
 * cannot be picked up or dropped mid-drag by the group's own displacement.
 */
export function moveSelection(
  doc: CanvasDoc,
  selected: Set<string>,
  dx: number,
  dy: number,
): CanvasDoc {
  const moving = withGroupMembers(doc, selected);
  return updateNodes(doc, moving, (n) => ({ x: n.x + dx, y: n.y + dy }));
}

/** A selection plus everything geometrically inside any selected group. */
export function withGroupMembers(doc: CanvasDoc, selected: Set<string>): Set<string> {
  const result = new Set(selected);
  for (const id of selected) {
    const node = doc.nodes.find((n) => n.id === id);
    if (node?.type !== 'group') continue;
    for (const member of groupMembers(doc, node as GroupNode)) {
      result.add(member.id);
    }
  }
  return result;
}

/** Resize a node, refusing to invert it. */
export const MIN_NODE_SIZE = 40;
export function resizeNode(
  doc: CanvasDoc,
  id: string,
  rect: Rect,
): CanvasDoc {
  return updateNode(doc, id, {
    x: rect.x,
    y: rect.y,
    width: Math.max(MIN_NODE_SIZE, rect.width),
    height: Math.max(MIN_NODE_SIZE, rect.height),
  });
}

/** Ids of nodes intersecting a marquee rectangle, in document order. */
export function nodesInRect(doc: CanvasDoc, rect: Rect): string[] {
  return doc.nodes.filter((n) => rectsIntersect(nodeRect(n), rect)).map((n) => n.id);
}

/**
 * Wrap a selection in a new group.
 *
 * The group is *prepended* — document order is z-order, and a backdrop drawn
 * last would cover the very cards it contains.
 */
export const GROUP_PADDING = 32;
export function groupSelection(
  doc: CanvasDoc,
  selected: Set<string>,
  id: string,
  label?: string,
): CanvasDoc {
  const members = doc.nodes.filter((n) => selected.has(n.id));
  const bounds = boundsOf(members);
  if (!bounds) return doc;

  const group: GroupNode = {
    id,
    type: 'group',
    x: bounds.x - GROUP_PADDING,
    y: bounds.y - GROUP_PADDING,
    width: bounds.width + GROUP_PADDING * 2,
    height: bounds.height + GROUP_PADDING * 2 + GROUP_PADDING,
    label,
  };

  return { ...doc, nodes: [group, ...doc.nodes] };
}

/** Duplicate nodes, offset so the copies are visibly distinct. */
export const DUPLICATE_OFFSET = 24;
export function duplicateNodes(
  doc: CanvasDoc,
  ids: Set<string>,
  newId: (index: number) => string,
): { doc: CanvasDoc; created: string[] } {
  const created: string[] = [];
  const copies = doc.nodes
    .filter((n) => ids.has(n.id))
    .map((n, i) => {
      const id = newId(i);
      created.push(id);
      return { ...n, id, x: n.x + DUPLICATE_OFFSET, y: n.y + DUPLICATE_OFFSET } as CanvasNode;
    });

  return { doc: { ...doc, nodes: [...doc.nodes, ...copies] }, created };
}

// ===========================================================================
// Undo history
// ===========================================================================

export interface History {
  past: CanvasDoc[];
  present: CanvasDoc;
  future: CanvasDoc[];
}

/**
 * How many steps are retained.
 *
 * Snapshots of a canvas are small (a few hundred nodes of plain data), so depth
 * costs little and running out of undo mid-rearrangement is far more annoying
 * than the memory.
 */
export const HISTORY_LIMIT = 100;

export function initHistory(doc: CanvasDoc): History {
  return { past: [], present: doc, future: [] };
}

/**
 * Whether two documents describe the same scene.
 *
 * Reference equality is not enough: every mutation helper returns a fresh
 * object, so a drag that resolves to zero movement — a click with a few pixels
 * of jitter, which snapping rounds to nothing — still produces a new document
 * that is structurally identical to the old one.
 */
export function sameDocument(a: CanvasDoc, b: CanvasDoc): boolean {
  if (a === b) return true;
  if (a.nodes.length !== b.nodes.length || a.edges.length !== b.edges.length) return false;
  return a.nodes.every((node, i) => {
    const other = b.nodes[i];
    return (
      node === other ||
      (node.id === other.id &&
        node.x === other.x &&
        node.y === other.y &&
        node.width === other.width &&
        node.height === other.height)
    );
  });
}

/**
 * Commit a new state.
 *
 * A commit that changes nothing is dropped rather than pushed: a click that
 * moves a card by zero pixels must not cost the user an undo step that appears
 * to do nothing when they press it.
 */
export function commit(history: History, next: CanvasDoc): History {
  if (sameDocument(history.present, next)) return history;

  const past = [...history.past, history.present];
  return {
    past: past.length > HISTORY_LIMIT ? past.slice(past.length - HISTORY_LIMIT) : past,
    present: next,
    future: [],
  };
}

export function undo(history: History): History {
  const previous = history.past[history.past.length - 1];
  if (previous === undefined) return history;
  return {
    past: history.past.slice(0, -1),
    present: previous,
    future: [history.present, ...history.future],
  };
}

export function redo(history: History): History {
  const [next, ...rest] = history.future;
  if (next === undefined) return history;
  return { past: [...history.past, history.present], present: next, future: rest };
}

export function canUndo(history: History): boolean {
  return history.past.length > 0;
}
export function canRedo(history: History): boolean {
  return history.future.length > 0;
}
