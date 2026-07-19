/**
 * The "roster" of browsable roots for the file-tree explorer's root dropdown.
 * A root is EITHER a Project (code/git dir, lazily walked) OR a Kiln (notes
 * dir, tree built client-side). Kilns and Projects are independent — a kiln
 * need not belong to a project.
 */
import type { Project, KilnListEntry } from '@/lib/types';
import { kilnRoot } from '@/lib/note-actions';

export type { KilnListEntry } from '@/lib/types';

export type TreeRootKind = 'project' | 'kiln';

export interface TreeRoot {
  kind: TreeRootKind;
  /** Absolute root path (project root, or kilnRoot()-normalized kiln root). */
  path: string;
  name: string;
}

export interface RosterGroup {
  label: 'Projects' | 'Kilns';
  kind: TreeRootKind;
  roots: TreeRoot[];
}

/** Stable persisted identity. Includes `kind` so a project and a same-path kiln never collide. */
export function rootKey(r: Pick<TreeRoot, 'kind' | 'path'>): string {
  return `${r.kind}:${r.path}`;
}

function basename(p: string): string {
  const parts = p.replace(/\/$/, '').split('/');
  return parts[parts.length - 1] || p;
}

/**
 * Build the two-group roster.
 *  - Projects: one root per registered project.
 *  - Kilns: the union of `GET /api/kilns` and every project's attached kilns,
 *    deduped by kiln ROOT path (`kilnRoot()` normalizes a `.crucible` config
 *    dir to its parent). A project and its attached kiln are DIFFERENT roots
 *    and never dedup against each other (`rootKey` includes `kind`).
 *
 * Order is preserved (kilns in first-seen order); empty groups are still
 * returned (callers filter for display).
 */
export function buildRoster(projects: Project[], kilns: KilnListEntry[]): RosterGroup[] {
  const projectRoots: TreeRoot[] = projects.map((p) => ({
    kind: 'project',
    path: p.path,
    name: p.name || basename(p.path),
  }));

  const seen = new Map<string, TreeRoot>();
  const pushKiln = (rawPath: string, name: string | null) => {
    const root = kilnRoot(rawPath);
    if (!root || seen.has(root)) return;
    seen.set(root, { kind: 'kiln', path: root, name: name?.trim() || basename(root) });
  };

  for (const k of kilns) pushKiln(k.path, k.name);
  for (const p of projects) for (const k of p.kilns) pushKiln(k.path, k.name);

  return [
    { label: 'Projects', kind: 'project', roots: projectRoots },
    { label: 'Kilns', kind: 'kiln', roots: [...seen.values()] },
  ];
}

/** Flat `rootKey -> TreeRoot` index for resolving a persisted/selected key. */
export function rosterIndex(groups: RosterGroup[]): Map<string, TreeRoot> {
  const idx = new Map<string, TreeRoot>();
  for (const g of groups) for (const r of g.roots) idx.set(rootKey(r), r);
  return idx;
}
