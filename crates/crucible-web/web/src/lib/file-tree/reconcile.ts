/**
 * Pure live-event reconciler + burst batcher for the file tree.
 *
 * The daemon emits absolute-path filesystem events; this module maps them onto
 * the in-memory `FileTreeNode` model:
 * Both kiln and project trees re-read affected loaded folders. Filesystem
 * events name paths, so a listing supplies the file kind and metadata.
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
 * For every filesystem tree: for each affected absolute path (both endpoints of a
 * `moved`, deduped), return the relPath of its parent folder IF that folder is
 * already loaded (`isDir && children !== undefined`). Callers re-issue
 * `listDir(root, relPath)` and swap children. Unloaded/missing folders are
 * ignored. Root (`''`) counts as loaded.
 */
export function foldersToInvalidate(mount: RootMount, events: FsEvent[]): string[] {
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
  return { invalidate: foldersToInvalidate(mount, events) };
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
