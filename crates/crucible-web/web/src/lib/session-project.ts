/**
 * Which project a session belongs to.
 *
 * A session has no project field; it has a workspace. A workspace is inside
 * exactly one registered project directory, so the directory answers it.
 * A session with no workspace (a tools-only agent) belongs to no project and
 * therefore matches none — it must not fall into whichever project happens to
 * be pinned.
 */
import type { Project, Session } from '@/lib/types';
import { sessionWorkspace } from '@/lib/session-scope';

function trimSlash(p: string): string {
  return p.replace(/\/+$/, '');
}

/** True when `session` acts inside `project`'s directory. */
export function sessionInProject(
  session: Pick<Session, 'workspace'>,
  project: Pick<Project, 'path'> | null,
): boolean {
  const workspace = sessionWorkspace(session);
  if (!workspace || !project) return false;
  const root = trimSlash(project.path);
  const ws = trimSlash(workspace);
  return ws === root || ws.startsWith(root + '/');
}

/** The registered project a session acts in, or null. Deepest path wins, so a
 * project registered inside another claims its own sessions. */
export function projectOfSession(
  session: Pick<Session, 'workspace'>,
  projects: readonly Project[],
): Project | null {
  let best: Project | null = null;
  for (const p of projects) {
    if (!sessionInProject(session, p)) continue;
    if (!best || trimSlash(p.path).length > trimSlash(best.path).length) best = p;
  }
  return best;
}
