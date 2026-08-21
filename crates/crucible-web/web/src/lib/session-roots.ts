/**
 * The roots a session can browse, and which one its file tree shows.
 *
 * The tree used to browse whatever root the user last picked, app-wide and
 * independent of the session — so splitting the session list out from the tree
 * would have been cosmetic: two panels side by side still showing unrelated
 * things. The tree now follows the ACTIVE SESSION.
 *
 * A session reaches more than one directory, which is why this is a list and
 * not a single cwd: it acts in a `workspace`, and it queries a flat set of
 * `kilns` where no member is privileged. Both are browsable, so both are
 * offered — the session's own roots first, then every other registered kiln,
 * because "browse a corpus this session cannot query yet" is a real thing to
 * want and the alternative is a picker that hides most of the machine.
 */
import type { KilnListEntry, Project, Session } from '@/lib/types';
import { kilnPathForName } from '@/lib/kiln-registry';
import { sessionWorkspace } from '@/lib/session-scope';
import { rootKey, type TreeRoot } from '@/lib/tree-root';

/**
 * Where a root came from — and, for a kiln, whether the session can already
 * query it. `other-kiln` is browsable but NOT attached: the agent cannot read
 * or cite it until the user attaches it deliberately.
 */
export type RootOrigin = 'workspace' | 'attached-kiln' | 'other-kiln';

export interface SessionRoot extends TreeRoot {
  origin: RootOrigin;
}

export interface SessionRoots {
  /** The session's own roots: its workspace, then its attached kilns. */
  own: SessionRoot[];
  /** Every registered kiln the session is not attached to. */
  others: SessionRoot[];
}

function basename(p: string): string {
  const parts = p.replace(/\/+$/, '').split('/');
  return parts[parts.length - 1] || p;
}

/**
 * Name a workspace the way the user registered it.
 *
 * A workspace inside a registered project borrows that project's name; a
 * git worktree therefore reads as its own lane rather than as a second
 * directory with the same basename as its main checkout.
 */
function workspaceName(path: string, projects: readonly Project[]): string {
  const trimmed = path.replace(/\/+$/, '');
  const exact = projects.find((p) => p.path.replace(/\/+$/, '') === trimmed);
  return exact?.name || basename(path);
}

/**
 * Split every browsable root into the session's own and the rest.
 *
 * An attached kiln whose NAME the registry cannot resolve is dropped, not
 * rendered as a nameless row: `kilnPathForName` answers `null` for an unknown
 * name, and `null` is not a root. Coercing it to `''` would point the tree at
 * the daemon data dir — a far wider corpus than the one the session attached.
 */
export function sessionRoots(
  session: Pick<Session, 'kilns' | 'workspace'> | null,
  kilns: readonly KilnListEntry[],
  projects: readonly Project[] = [],
): SessionRoots {
  const own: SessionRoot[] = [];
  const attached = new Set<string>();

  const workspace = session ? sessionWorkspace(session) : null;
  if (workspace) {
    own.push({
      kind: 'project',
      path: workspace,
      name: workspaceName(workspace, projects),
      origin: 'workspace',
    });
  }

  for (const name of session?.kilns ?? []) {
    const path = kilnPathForName(name, kilns);
    if (!path) continue;
    attached.add(name);
    own.push({ kind: 'kiln', path, name, origin: 'attached-kiln' });
  }

  const others: SessionRoot[] = kilns
    .filter((k) => k.name && !attached.has(k.name))
    .map((k) => ({
      kind: 'kiln' as const,
      path: k.path,
      name: k.name!,
      origin: 'other-kiln' as const,
    }));

  return { own, others };
}

/**
 * The root the tree shows: the session's pin when it still resolves, else the
 * session's first own root.
 *
 * The pin is checked against BOTH lists, so a kiln the user pinned while only
 * browsing it survives a switch away and back. It is dropped when it resolves
 * to nothing — an unregistered kiln, or a workspace the session no longer has
 * — rather than leaving the tree pointed at a directory this session cannot
 * reach.
 */
export function resolveSessionRoot(
  roots: SessionRoots,
  pinnedKey: string | null,
): SessionRoot | null {
  if (pinnedKey) {
    const pinned = [...roots.own, ...roots.others].find((r) => rootKey(r) === pinnedKey);
    if (pinned) return pinned;
  }
  return roots.own[0] ?? null;
}
