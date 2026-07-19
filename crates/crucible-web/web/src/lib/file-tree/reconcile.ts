/**
 * Pure live-event reconciler + burst batcher for the file tree.
 *
 * The daemon emits absolute-path filesystem events; this module maps them onto
 * the in-memory `FileTreeNode` model:
 *  - Kilns (fully built, markdown-only) are patched in place — new markdown
 *    leaves are added (synthesizing missing directory nodes), removed leaves
 *    are dropped. Non-`.md` events are ignored.
 *  - Projects (lazily loaded) are never mutated here; instead we return the set
 *    of already-loaded parent folders that need a fresh `listDir`. In Phase 1
 *    `file_*` events only fire for watched kiln dirs, so the project branch is
 *    exercised by unit tests only (kept for the Task 4e follow-up).
 *
 * `moved` is decomposed into remove(from) + add(to), so a platform that emits
 * `deleted` + `changed{created}` instead converges to the same tree —
 * idempotent and order-independent.
 */
import type { FsEvent } from '@/lib/types';
import type { FileTreeNode } from './types';

export interface RootMount {
  rootId: string;
  kind: 'kiln' | 'project';
  /** Absolute root path (kiln root or project root). */
  basePath: string;
  root: FileTreeNode;
}

/** An atomic filesystem mutation after `moved` is decomposed. */
type AtomicOp = { op: 'add' | 'remove'; path: string };

const stripTrailingSlash = (p: string): string => p.replace(/\/+$/, '');

/**
 * Which mount owns `absPath`, and the mount-root-relative path parts. Prefix
 * matching is path-segment aware: `/vault` does not own `/vault2/x`.
 */
export function locate(
  mounts: RootMount[],
  absPath: string,
): { rootId: string; relParts: string[] } | null {
  for (const m of mounts) {
    const base = stripTrailingSlash(m.basePath);
    if (absPath === base) return { rootId: m.rootId, relParts: [] };
    if (absPath.startsWith(base + '/')) {
      const rel = absPath.slice(base.length + 1);
      return { rootId: m.rootId, relParts: rel.split('/').filter(Boolean) };
    }
  }
  return null;
}

/** Decompose one event into its atomic add/remove ops (paths absolute). */
function decompose(event: FsEvent): AtomicOp[] {
  switch (event.type) {
    case 'changed':
      return [{ op: 'add', path: event.path }];
    case 'deleted':
      return [{ op: 'remove', path: event.path }];
    case 'moved':
      return [
        { op: 'remove', path: event.from },
        { op: 'add', path: event.to },
      ];
  }
}

/** Locate an existing node by its root-relative path (`''` => the root). */
export function findNodeByRelPath(root: FileTreeNode, relPath: string): FileTreeNode | null {
  if (relPath === '') return root;
  let node: FileTreeNode | null = root;
  for (const seg of relPath.split('/')) {
    if (!node?.children) return null;
    node = node.children.find((c) => c.name === seg) ?? null;
    if (!node) return null;
  }
  return node;
}

/**
 * Kiln patch: apply every event's markdown add/remove ops to a COPY of the
 * tree and return the new root. Missing intermediate directories are
 * synthesized on add; ancestors are never auto-pruned on remove.
 */
export function reconcileKilnTree(
  root: FileTreeNode,
  basePath: string,
  events: FsEvent[],
): FileTreeNode {
  const base = stripTrailingSlash(basePath);
  let next = root;

  for (const event of events) {
    for (const { op, path } of decompose(event)) {
      if (!path.endsWith('.md')) continue; // kilns are markdown-only
      const owned = locate([{ rootId: 'k', kind: 'kiln', basePath: base, root: next }], path);
      if (!owned || owned.relParts.length === 0) continue;
      next =
        op === 'add'
          ? addLeaf(next, owned.relParts, base)
          : removeLeaf(next, owned.relParts);
    }
  }
  return next;
}

/** Immutably insert a markdown leaf at `relParts`, synthesizing dir nodes. */
function addLeaf(root: FileTreeNode, relParts: string[], base: string): FileTreeNode {
  const [head, ...rest] = relParts;
  const children = root.children ? [...root.children] : [];
  const childRel = joinRel(root.relPath, head);

  if (rest.length === 0) {
    if (children.some((c) => !c.isDir && c.name === head)) return root; // already present
    children.push({
      relPath: childRel,
      name: head,
      isDir: false,
      absPath: `${base}/${childRel}`,
    });
    return { ...root, children };
  }

  const idx = children.findIndex((c) => c.isDir && c.name === head);
  const existing =
    idx >= 0
      ? children[idx]
      : ({
          relPath: childRel,
          name: head,
          isDir: true,
          absPath: `${base}/${childRel}`,
          children: [],
        } satisfies FileTreeNode);
  const updated = addLeaf(existing, rest, base);
  if (idx >= 0) children[idx] = updated;
  else children.push(updated);
  return { ...root, children };
}

/** Immutably remove the exact leaf at `relParts`; keep empty ancestor dirs. */
function removeLeaf(root: FileTreeNode, relParts: string[]): FileTreeNode {
  const [head, ...rest] = relParts;
  if (!root.children) return root;

  if (rest.length === 0) {
    const children = root.children.filter((c) => !(!c.isDir && c.name === head));
    if (children.length === root.children.length) return root; // nothing removed
    return { ...root, children };
  }

  const idx = root.children.findIndex((c) => c.isDir && c.name === head);
  if (idx < 0) return root;
  const children = [...root.children];
  children[idx] = removeLeaf(children[idx], rest);
  return { ...root, children };
}

const joinRel = (prefix: string, seg: string): string => (prefix ? `${prefix}/${seg}` : seg);

/**
 * Project defensive path: for each affected absolute path (both endpoints of a
 * `moved`, deduped), return the relPath of its parent folder IF that folder is
 * already loaded (`isDir && children !== undefined`). Callers re-issue
 * `listDir(root, relPath)` and swap children. Unloaded/missing folders are
 * ignored. Root (`''`) counts as loaded.
 */
export function projectFoldersToInvalidate(mount: RootMount, events: FsEvent[]): string[] {
  const base = stripTrailingSlash(mount.basePath);
  const out = new Set<string>();

  const affectedPaths = events.flatMap((e) =>
    e.type === 'moved' ? [e.from, e.to] : [e.path],
  );

  for (const abs of affectedPaths) {
    const owned = locate([{ ...mount, basePath: base }], abs);
    if (!owned || owned.relParts.length === 0) continue;
    const parentRel = owned.relParts.slice(0, -1).join('/');
    const parent = findNodeByRelPath(mount.root, parentRel);
    if (parent && parent.isDir && parent.children !== undefined) {
      out.add(parentRel);
    }
  }
  return [...out];
}

/** Reconcile a single mounted root against a batch of events. */
export function reconcileMount(
  mount: RootMount,
  events: FsEvent[],
): { root?: FileTreeNode; invalidate?: string[] } {
  if (mount.kind === 'kiln') {
    return { root: reconcileKilnTree(mount.root, mount.basePath, events) };
  }
  return { invalidate: projectFoldersToInvalidate(mount, events) };
}

export interface FsEventBatcher {
  push(event: FsEvent): void;
  /** Force an immediate flush (e.g. on teardown/tests). */
  flush(): void;
  dispose(): void;
}

/**
 * Coalesce a burst of events into one `onFlush` call after `flushMs` of quiet.
 * The daemon's 500 ms debounce already caps the per-file rate; this batches
 * across files so one reconcile pass covers a burst.
 */
export function createFsEventBatcher(
  flushMs: number,
  onFlush: (events: FsEvent[]) => void,
): FsEventBatcher {
  let pending: FsEvent[] = [];
  let timer: ReturnType<typeof setTimeout> | null = null;

  const flush = () => {
    if (timer) {
      clearTimeout(timer);
      timer = null;
    }
    if (pending.length === 0) return;
    const batch = pending;
    pending = [];
    onFlush(batch);
  };

  return {
    push(event) {
      pending.push(event);
      if (!timer) timer = setTimeout(flush, flushMs);
    },
    flush,
    dispose() {
      if (timer) clearTimeout(timer);
      timer = null;
      pending = [];
    },
  };
}
